pub mod app;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    /// The namespace in which the workload gets created
    pub target_namespace: String,

    #[serde(default)]
    pub template: DeploymentTemplate,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DeploymentTemplate {
    #[serde(default)]
    pub image: Option<String>,

    #[serde(default)]
    pub image_pull_policy: Option<String>,
}
