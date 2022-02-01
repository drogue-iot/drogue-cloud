use drogue_cloud_endpoint_common::{auth::AuthConfig, command::KafkaCommandSourceConfig};
use drogue_cloud_mqtt_common::server::{MqttServerOptions, TlsConfig};
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::{defaults, health::HealthServerConfig};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct EndpointConfig {
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
    #[serde(default = "default_cache_duration")]
    #[serde(with = "humantime_serde")]
    pub cache_duration: Duration,
}

const fn default_cache_size() -> usize {
    128
}

const fn default_cache_duration() -> Duration {
    Duration::from_secs(30)
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            cache_duration: default_cache_duration(),
            cache_size: default_cache_size(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub disable_tls: bool,

    #[serde(default)]
    pub disable_client_certificates: bool,

    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,

    #[serde(default)]
    pub mqtt: MqttServerOptions,

    #[serde(default)]
    pub endpoint: EndpointConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub auth: AuthConfig,

    pub command_source_kafka: KafkaCommandSourceConfig,

    pub kafka_downstream_config: KafkaClientConfig,
    pub kafka_command_config: KafkaClientConfig,

    pub instance: String,

    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,
}

impl TlsConfig for Config {
    fn is_disabled(&self) -> bool {
        self.disable_tls
    }

    fn disable_client_certs(&self) -> bool {
        self.disable_client_certificates
    }

    #[cfg(feature = "rustls")]
    fn verifier_rustls(&self) -> std::sync::Arc<dyn rust_tls::server::ClientCertVerifier> {
        // This seems dangerous, as we simply accept all client certificates. However,
        // we validate them later during the "connect" packet validation.
        std::sync::Arc::new(crate::auth::AcceptAllClientCertVerifier)
    }

    fn key_file(&self) -> Option<&str> {
        self.key_file.as_deref()
    }

    fn cert_bundle_file(&self) -> Option<&str> {
        self.cert_bundle_file.as_deref()
    }
}
