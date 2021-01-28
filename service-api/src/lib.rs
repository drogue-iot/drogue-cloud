pub mod auth;
pub mod management;

use serde::Deserialize;
use serde_json::{Map, Value};

pub trait Translator {
    fn spec(&self) -> &Map<String, Value>;
    fn status(&self) -> &Map<String, Value>;

    fn spec_as<T, S>(&self, key: S) -> Option<Result<T, serde_json::Error>>
    where
        T: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        let result = self
            .spec()
            .get(key.as_ref())
            .map(|value| serde_json::from_value(value.clone()));

        result
    }

    fn status_as<T, S>(&self, key: S) -> Option<Result<T, serde_json::Error>>
    where
        T: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        let result = self
            .status()
            .get(key.as_ref())
            .map(|value| serde_json::from_value(value.clone()));

        result
    }
}
