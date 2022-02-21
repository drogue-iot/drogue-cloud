use serde::Deserialize;
use std::collections::HashMap;

pub trait ConfigFromEnv<'de>: Sized + Deserialize<'de> {
    fn from_env() -> Result<Self, config::ConfigError> {
        Self::from(config::Environment::default())
    }

    fn from_env_prefix<S: AsRef<str>>(prefix: S) -> Result<Self, config::ConfigError> {
        Self::from(config::Environment::with_prefix(prefix.as_ref()))
    }

    fn from(env: config::Environment) -> Result<Self, config::ConfigError>;

    fn from_set<K, V>(set: HashMap<K, V>) -> Result<Self, config::ConfigError>
    where
        K: Into<String>,
        V: Into<String>,
    {
        let set = set.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        Self::from(config::Environment::default().source(Some(set)))
    }
}

impl<'de, T: Deserialize<'de> + Sized> ConfigFromEnv<'de> for T {
    fn from(env: config::Environment) -> Result<T, config::ConfigError> {
        let env = env.try_parsing(true).separator("__");

        let cfg = config::Config::builder().add_source(env);
        cfg.build()?.try_deserialize()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use config::Environment;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[test]
    fn test_prefix() {
        #[derive(Debug, Deserialize)]
        struct Foo {
            pub bar: String,
            pub r#bool: bool,
        }

        let mut env = HashMap::<String, String>::new();
        env.insert("FOO__BAR".into(), "baz".into());
        env.insert("FOO__BOOL".into(), "true".into());

        let foo = <Foo as ConfigFromEnv>::from(Environment::with_prefix("FOO").source(Some(env)))
            .unwrap();
        assert_eq!(foo.bar, "baz");
        assert_eq!(foo.r#bool, true);
    }

    #[test]
    fn test_nested() {
        #[derive(Debug, Deserialize)]
        struct Foo {
            #[serde(default)]
            pub bar: Option<Bar>,
        }
        #[derive(Debug, Deserialize)]
        struct Bar {
            pub baz: Baz,
        }
        #[derive(Debug, Deserialize)]
        struct Baz {
            pub value: String,
        }

        let mut env = HashMap::<String, String>::new();
        env.insert("FOO__BAR__BAZ__VALUE".into(), "s1".into());

        let foo =
            <Foo as ConfigFromEnv>::from(Environment::default().prefix("FOO").source(Some(env)))
                .unwrap();

        assert_eq!(foo.bar.unwrap().baz.value, "s1");
    }
}
