pub mod app;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    /// The namespace in which the topics get created
    pub topic_namespace: String,
    /// The resource name of the Kafka cluster.
    ///
    /// This will be used as the `strimzi.io/cluster` label value.
    pub cluster_name: String,
}
