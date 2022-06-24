use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SERVICE_CA_CERT: &str = "/var/run/secrets/kubernetes.io/serviceaccount/service-ca.crt";

/// A client configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default)]
    pub tls_insecure: bool,
    #[serde(default)]
    pub ca_certificates: Vec<String>,
    /// A second way to provide a single certificate, works with the config crate, which
    /// doesn't properly support lists.
    #[serde(default)]
    pub ca_certificate: Option<String>,
}

impl ClientConfig {
    pub fn certificates(&self) -> impl Iterator<Item = &str> {
        let service_ca = {
            let cert = Path::new(SERVICE_CA_CERT);
            if cert.exists() {
                log::info!("Adding Service CA certificate ({:?})", cert);
                Some(SERVICE_CA_CERT)
            } else {
                None
            }
        };

        self.ca_certificates
            .iter()
            .map(|s| s.as_str())
            .chain(self.ca_certificate.iter().map(|s| s.as_str()))
            .chain(service_ca.into_iter())
    }
}

#[cfg(feature = "native-tls")]
impl TryFrom<&ClientConfig> for native_tls::TlsConnector {
    type Error = anyhow::Error;

    fn try_from(config: &ClientConfig) -> Result<Self, Self::Error> {
        use anyhow::Context;

        let mut tls = native_tls::TlsConnector::builder();

        if config.tls_insecure {
            log::warn!("Disabling TLS verification for client. Do not use this in production!");
            tls.danger_accept_invalid_certs(true);
            tls.danger_accept_invalid_hostnames(true);
        }

        for cert in config.certificates() {
            let cert = std::fs::read(cert).context("Reading certificate")?;
            let cert = native_tls::Certificate::from_pem(&cert)?;
            tls.add_root_certificate(cert);
        }

        Ok(tls.build().context("Create TLS connector")?)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::config::ConfigFromEnv;
    use config::Environment;
    use std::collections::HashMap;

    #[test]
    fn test_ca_cert() {
        let mut env = HashMap::<String, String>::new();
        env.insert("FOO__CA_CERTIFICATE".into(), "/path/to/file".into());

        let config = <ClientConfig as ConfigFromEnv>::from(
            Environment::default().prefix("FOO").source(Some(env)),
        )
        .unwrap();

        assert_eq!(
            config,
            ClientConfig {
                tls_insecure: false,
                ca_certificates: vec![],
                ca_certificate: Some("/path/to/file".to_string()),
            }
        );

        assert_eq!(
            config.certificates().collect::<Vec<_>>(),
            vec!["/path/to/file"]
        )
    }
}
