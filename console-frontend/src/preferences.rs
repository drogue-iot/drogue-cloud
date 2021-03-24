use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use yew::format::Json;
use yew::services::storage::*;

const KEY: &str = "preferences";

/// User preferences, stored in the local browser.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

impl Preferences {
    fn storage() -> anyhow::Result<StorageService> {
        Ok(StorageService::new(Area::Local).map_err(|err| anyhow!(err))?)
    }

    /// Store preferences to local store.
    pub fn store(&self) -> anyhow::Result<()> {
        let mut storage = Self::storage()?;
        storage.store(KEY, Json(self));
        Ok(())
    }

    /// Load preferences from local store.
    pub fn load() -> anyhow::Result<Preferences> {
        let storage = Self::storage()?;
        storage.restore::<Json<anyhow::Result<Preferences>>>(KEY).0
    }

    /// A function to conveniently load, update, and store preferences.
    pub fn update<F>(f: F) -> anyhow::Result<()>
    where
        F: FnOnce(Option<Preferences>) -> anyhow::Result<Preferences>,
    {
        f(Self::load().ok())?.store()?;
        Ok(())
    }

    /// A function to conveniently load, update, and store preferences.
    ///
    /// Compared to `update` the function will create a new default preference instance
    /// when none could be restored.
    pub fn update_or_default<F>(f: F) -> anyhow::Result<()>
    where
        F: FnOnce(Preferences) -> anyhow::Result<Preferences>,
    {
        Self::update(|prefs| f(prefs.unwrap_or_default()))
    }
}
