use drogue_client::{Dialect, Section};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtnAppSpec {
    pub api: TtnAppApi,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtnAppApi {
    pub api_key: String,
    pub region: RegionOrUrl,
    // FIXME: allow using an org as well
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RegionOrUrl {
    Url(Url),
    Region(String),
}

impl RegionOrUrl {
    pub fn url(&self) -> Result<Url, anyhow::Error> {
        match self {
            Self::Url(url) => Ok(url.clone()),
            Self::Region(region) => Ok(Url::parse(&format!(
                "https://{}.cloud.thethings.network",
                region
            ))?),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtnAppStatus {
    pub observed_generation: u64,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

impl Dialect for TtnAppSpec {
    fn key() -> &'static str {
        "ttn"
    }

    fn section() -> Section {
        Section::Spec
    }
}

impl Dialect for TtnAppStatus {
    fn key() -> &'static str {
        "ttn"
    }

    fn section() -> Section {
        Section::Status
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use serde_json::json;

    #[test]
    fn test_region_str() {
        let api: TtnAppApi =
            serde_json::from_value(json!({"api_key": "foo", "region": "bar"})).unwrap();

        assert_eq!(api.api_key, "foo");
        assert_eq!(api.region, RegionOrUrl::Region("bar".into()));
    }
}
