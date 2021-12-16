use drogue_cloud_console_common::UserInfo;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const KEY: &str = "preferences";

/// User preferences, stored in the local browser.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_info: Option<UserInfo>,
}

impl Preferences {
    /// Store preferences to local store.
    pub fn store(&self) -> anyhow::Result<()> {
        LocalStorage::set(KEY, self)?;
        Ok(())
    }

    /// Load preferences from local store.
    pub fn load() -> anyhow::Result<Preferences> {
        Ok(LocalStorage::get(KEY)?)
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
