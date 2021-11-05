// Copyright 2021 The Matrix.org Foundation C.I.C.
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

use hyper::Method;
use mas_config::OAuth2Config;
use warp::{Filter, Rejection, Reply};

use crate::filters::cors::cors;

pub(super) fn filter(
    config: &OAuth2Config,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + Sync + 'static {
    let jwks = config.keys.to_public_jwks();

    warp::path!("oauth2" / "keys.json").and(
        warp::get()
            .map(move || warp::reply::json(&jwks))
            .with(cors().allow_method(Method::GET)),
    )
}