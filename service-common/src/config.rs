use serde::Deserialize;

pub trait ConfigFromEnv<'de>: Sized + Deserialize<'de> {
    fn from_env() -> Result<Self, config::ConfigError> {
        Self::from(config::Environment::new())
    }

    fn from_env_prefix<S: AsRef<str>>(prefix: S) -> Result<Self, config::ConfigError> {
        Self::from(config::Environment::with_prefix(&format!(
            "{}_",
            prefix.as_ref()
        )))
    }

    fn from(env: config::Environment) -> Result<Self, config::ConfigError>;
}

impl<'de, T: Deserialize<'de> + Sized> ConfigFromEnv<'de> for T {
    fn from(env: config::Environment) -> Result<T, config::ConfigError> {
        let mut cfg = config::Config::new();
        cfg.merge(env.separator("__"))?;
        cfg.try_into()
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use serde::Deserialize;

    #[test]
    fn test_prefix() {
        #[derive(Debug, Deserialize)]
        struct Foo {
            pub bar: String,
        }

        std::env::set_var("FOO__BAR", "baz");

        let foo = Foo::from_env_prefix("FOO").unwrap();
        assert_eq!(foo.bar, "baz");

        std::env::remove_var("FOO__BAR");
    }
}
