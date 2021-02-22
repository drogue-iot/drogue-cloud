use serde::Deserialize;

pub trait ConfigFromEnv<'de>: Sized + Deserialize<'de> {
    fn from_env() -> Result<Self, config::ConfigError> {
        let mut cfg = config::Config::new();
        cfg.merge(config::Environment::new().separator("__"))?;
        cfg.try_into()
    }
}
