// Copyright 2022 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{collections::HashMap, error::Error};

use async_trait::async_trait;
use axum::{
    body::HttpBody,
    extract::{
        rejection::{FailedToDeserializeQueryString, FormRejection, TypedHeaderRejectionReason},
        Form, FromRequest, TypedHeader,
    },
    response::{IntoResponse, Response},
};
use headers::{authorization::Bearer, Authorization, Header, HeaderMapExt, HeaderName};
use http::{header::WWW_AUTHENTICATE, HeaderMap, HeaderValue, StatusCode};
use mas_data_model::Session;
use mas_storage::{
    oauth2::access_token::{lookup_active_access_token, AccessTokenLookupError},
    PostgresqlBackend,
};
use serde::{de::DeserializeOwned, Deserialize};
use sqlx::{Acquire, Postgres};

#[derive(Debug, Deserialize)]
struct AuthorizedForm<F> {
    #[serde(default)]
    access_token: Option<String>,

    #[serde(flatten)]
    inner: F,
}

#[derive(Debug)]
enum AccessToken {
    Form(String),
    Header(String),
    None,
}

impl AccessToken {
    pub async fn fetch(
        &self,
        conn: impl Acquire<'_, Database = Postgres> + Send,
    ) -> Result<
        (
            mas_data_model::AccessToken<PostgresqlBackend>,
            Session<PostgresqlBackend>,
        ),
        AuthorizationVerificationError,
    > {
        let token = match &self {
            AccessToken::Form(t) | AccessToken::Header(t) => t,
            AccessToken::None => return Err(AuthorizationVerificationError::MissingToken),
        };

        let (token, session) = lookup_active_access_token(conn, token).await?;

        Ok((token, session))
    }
}

#[derive(Debug)]
pub struct UserAuthorization<F = ()> {
    access_token: AccessToken,
    form: Option<F>,
}

impl<F> UserAuthorization<F> {
    // TODO: take scopes to validate as parameter
    pub async fn protected_form(
        self,
        conn: impl Acquire<'_, Database = Postgres> + Send,
    ) -> Result<(Session<PostgresqlBackend>, F), AuthorizationVerificationError> {
        let form = match self.form {
            Some(f) => f,
            None => return Err(AuthorizationVerificationError::MissingForm),
        };

        let (_token, session) = self.access_token.fetch(conn).await?;

        Ok((session, form))
    }

    // TODO: take scopes to validate as parameter
    pub async fn protected(
        self,
        conn: impl Acquire<'_, Database = Postgres> + Send,
    ) -> Result<Session<PostgresqlBackend>, AuthorizationVerificationError> {
        let (_token, session) = self.access_token.fetch(conn).await?;

        Ok(session)
    }
}

pub enum UserAuthorizationError {
    InvalidHeader,
    TokenInFormAndHeader,
    BadForm(FailedToDeserializeQueryString),
    InternalError(Box<dyn Error>),
}

pub enum AuthorizationVerificationError {
    MissingToken,
    InvalidToken,
    MissingForm,
    InternalError(Box<dyn Error>),
}

impl From<AccessTokenLookupError> for AuthorizationVerificationError {
    fn from(e: AccessTokenLookupError) -> Self {
        if e.not_found() {
            Self::InvalidToken
        } else {
            Self::InternalError(Box::new(e))
        }
    }
}

enum BearerError {
    InvalidRequest,
    InvalidToken,
    #[allow(dead_code)]
    InsufficientScope {
        scope: Option<HeaderValue>,
    },
}

impl BearerError {
    fn error(&self) -> HeaderValue {
        match self {
            BearerError::InvalidRequest => HeaderValue::from_static("invalid_request"),
            BearerError::InvalidToken => HeaderValue::from_static("invalid_token"),
            BearerError::InsufficientScope { .. } => HeaderValue::from_static("insufficient_scope"),
        }
    }

    fn params(&self) -> HashMap<&'static str, HeaderValue> {
        match self {
            BearerError::InsufficientScope { scope: Some(scope) } => {
                let mut m = HashMap::new();
                m.insert("scope", scope.clone());
                m
            }
            _ => HashMap::new(),
        }
    }
}

enum WwwAuthenticate {
    #[allow(dead_code)]
    Basic { realm: HeaderValue },
    Bearer {
        realm: Option<HeaderValue>,
        error: BearerError,
        error_description: Option<HeaderValue>,
    },
}

impl Header for WwwAuthenticate {
    fn name() -> &'static HeaderName {
        &WWW_AUTHENTICATE
    }

    fn decode<'i, I>(_values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i http::HeaderValue>,
    {
        Err(headers::Error::invalid())
    }

    fn encode<E: Extend<http::HeaderValue>>(&self, values: &mut E) {
        let (scheme, params) = match self {
            WwwAuthenticate::Basic { realm } => {
                let mut params = HashMap::new();
                params.insert("realm", realm.clone());
                ("Basic", params)
            }
            WwwAuthenticate::Bearer {
                realm,
                error,
                error_description,
            } => {
                let mut params = error.params();
                params.insert("error", error.error());

                if let Some(realm) = realm {
                    params.insert("realm", realm.clone());
                }

                if let Some(error_description) = error_description {
                    params.insert("error_description", error_description.clone());
                }

                ("Bearer", params)
            }
        };

        let params = params.into_iter().map(|(k, v)| format!(" {}={:?}", k, v));
        let value: String = std::iter::once(scheme.to_string()).chain(params).collect();
        let value = HeaderValue::from_str(&value).unwrap();
        values.extend(std::iter::once(value));
    }
}

impl IntoResponse for UserAuthorizationError {
    fn into_response(self) -> Response {
        match self {
            Self::BadForm(_) | Self::InvalidHeader | Self::TokenInFormAndHeader => {
                let mut headers = HeaderMap::new();

                headers.typed_insert(WwwAuthenticate::Bearer {
                    realm: None,
                    error: BearerError::InvalidRequest,
                    error_description: None,
                });
                (StatusCode::BAD_REQUEST, headers).into_response()
            }
            Self::InternalError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    }
}

impl IntoResponse for AuthorizationVerificationError {
    fn into_response(self) -> Response {
        match self {
            Self::MissingForm | Self::MissingToken => {
                let mut headers = HeaderMap::new();

                headers.typed_insert(WwwAuthenticate::Bearer {
                    realm: None,
                    error: BearerError::InvalidRequest,
                    error_description: None,
                });
                (StatusCode::BAD_REQUEST, headers).into_response()
            }
            Self::InvalidToken => {
                let mut headers = HeaderMap::new();

                headers.typed_insert(WwwAuthenticate::Bearer {
                    realm: None,
                    error: BearerError::InvalidToken,
                    error_description: None,
                });
                (StatusCode::BAD_REQUEST, headers).into_response()
            }
            Self::InternalError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    }
}

#[async_trait]
impl<B, F> FromRequest<B> for UserAuthorization<F>
where
    B: Send + HttpBody,
    B::Data: Send,
    B::Error: Error + Send + Sync + 'static,
    F: DeserializeOwned,
{
    type Rejection = UserAuthorizationError;

    async fn from_request(
        req: &mut axum::extract::RequestParts<B>,
    ) -> Result<Self, Self::Rejection> {
        let header = TypedHeader::<Authorization<Bearer>>::from_request(req).await;

        let token_from_header = match header {
            Ok(header) => Some(header.token().to_string()),
            Err(err) => match err.reason() {
                TypedHeaderRejectionReason::Missing => None,
                TypedHeaderRejectionReason::Error(_) => {
                    return Err(UserAuthorizationError::InvalidHeader)
                }
            },
        };

        let (token_from_form, form) = match Form::<AuthorizedForm<F>>::from_request(req).await {
            Ok(Form(form)) => (form.access_token, Some(form.inner)),
            Err(FormRejection::InvalidFormContentType(_err)) => (None, None),
            Err(FormRejection::FailedToDeserializeQueryString(err)) => {
                return Err(UserAuthorizationError::BadForm(err))
            }
            Err(e) => return Err(UserAuthorizationError::InternalError(Box::new(e))),
        };

        let access_token = match (token_from_header, token_from_form) {
            (Some(_), Some(_)) => return Err(UserAuthorizationError::TokenInFormAndHeader),
            (Some(t), None) => AccessToken::Header(t),
            (None, Some(t)) => AccessToken::Form(t),
            (None, None) => AccessToken::None,
        };

        Ok(UserAuthorization { access_token, form })
    }
}