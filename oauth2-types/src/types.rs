//! Utilitary types for serde

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, hash::Hash, time::Duration};

/// A HashSet that serializes to a space-separated string in alphanumerical order
#[derive(Debug, PartialEq)]
pub struct StringHashSet<T: Eq + Hash>(HashSet<T>);

impl<T: Eq + Hash> From<HashSet<T>> for StringHashSet<T> {
    fn from(set: HashSet<T>) -> Self {
        Self(set)
    }
}

impl<T: Eq + Hash> From<StringHashSet<T>> for HashSet<T> {
    fn from(set: StringHashSet<T>) -> Self {
        set.0
    }
}

impl<T> Serialize for StringHashSet<T>
where
    T: ToString + PartialOrd + Eq + Hash,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut items: Vec<_> = self.0.iter().map(|i| i.to_string()).collect();
        items.sort();
        let s = items.join(" ");
        serializer.serialize_str(&s)
    }
}

impl<'de, T> Deserialize<'de> for StringHashSet<T>
where
    T: std::str::FromStr + Eq + Hash,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let items: Result<HashSet<T>, _> = s.split_ascii_whitespace().map(T::from_str).collect();
        items.map(Into::into).map_err(serde::de::Error::custom)
    }
}

/// A Vec that serializes to a space-separated string
pub struct StringVec<T>(Vec<T>);

impl<T> From<Vec<T>> for StringVec<T> {
    fn from(set: Vec<T>) -> Self {
        Self(set)
    }
}

impl<T> From<StringVec<T>> for Vec<T> {
    fn from(v: StringVec<T>) -> Self {
        v.0
    }
}

impl<T> Serialize for StringVec<T>
where
    T: ToString,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let items: Vec<_> = self.0.iter().map(|i| i.to_string()).collect();
        let s = items.join(" ");
        serializer.serialize_str(&s)
    }
}

impl<'de, T> Deserialize<'de> for StringVec<T>
where
    T: std::str::FromStr + std::hash::Hash + Eq,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let items: Result<Vec<T>, _> = s.split_ascii_whitespace().map(T::from_str).collect();
        items.map(Into::into).map_err(serde::de::Error::custom)
    }
}

/// A Duration that serializes to seconds
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Seconds(Duration);

impl From<Duration> for Seconds {
    fn from(d: Duration) -> Self {
        Self(d)
    }
}

impl From<Seconds> for Duration {
    fn from(val: Seconds) -> Self {
        val.0
    }
}

impl Serialize for Seconds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_secs().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Seconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Self(Duration::from_secs(secs)))
    }
}