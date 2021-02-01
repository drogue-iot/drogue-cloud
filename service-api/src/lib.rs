pub mod auth;
pub mod management;

use serde::Deserialize;
use serde_json::{Map, Value};

pub trait Translator {
    fn spec(&self) -> &Map<String, Value>;
    fn status(&self) -> &Map<String, Value>;

    fn section<D>(&self) -> Option<Result<D, serde_json::Error>>
    where
        D: for<'de> Deserialize<'de> + Dialect,
    {
        match D::section() {
            Section::Spec => self.spec_for(D::key()),
            Section::Status => self.status_for(D::key()),
        }
    }

    fn spec_for<T, S>(&self, key: S) -> Option<Result<T, serde_json::Error>>
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

    fn status_for<T, S>(&self, key: S) -> Option<Result<T, serde_json::Error>>
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

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Section {
    Spec,
    Status,
}

pub trait Dialect {
    fn key() -> &'static str;
    fn section() -> Section;
}

#[cfg(test)]
mod test {

    use super::*;

    #[derive(Deserialize, Debug, Clone, Default)]
    pub struct Foo {
        pub spec: Map<String, Value>,
        pub status: Map<String, Value>,
    }

    impl Translator for Foo {
        fn spec(&self) -> &Map<String, Value> {
            &self.spec
        }

        fn status(&self) -> &Map<String, Value> {
            &self.status
        }
    }

    #[derive(Deserialize, Debug, Clone, Default)]
    pub struct Bar {
        pub name: String,
    }

    impl Dialect for Bar {
        fn key() -> &'static str {
            "bar"
        }

        fn section() -> Section {
            Section::Spec
        }
    }

    #[test]
    fn test1() {
        let i = Foo::default();
        let _: Option<Result<Bar, _>> = i.section::<Bar>();
    }
}
