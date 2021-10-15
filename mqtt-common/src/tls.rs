#[cfg(feature = "rustls")]
pub use rustls::tls_config as rustls_config;

#[cfg(feature = "rustls")]
mod rustls {
    use crate::server::TlsConfig;
    use anyhow::Context;
    use pem::parse_many;
    use rust_tls::{internal::pemfile::certs, PrivateKey, ServerConfig};
    use std::{fs::File, io::BufReader};

    /// Build a server config for rustls.
    pub fn tls_config(config: &dyn TlsConfig) -> anyhow::Result<ServerConfig> {
        let mut tls_config = ServerConfig::new(config.verifier());

        let key = config
            .key_file()
            .ok_or_else(|| anyhow::anyhow!("TLS configuration error: Missing key file"))?;
        let cert = config
            .cert_bundle_file()
            .ok_or_else(|| anyhow::anyhow!("TLS configuration error: Missing cert file"))?;

        let cert_file = &mut BufReader::new(File::open(cert).unwrap());
        let cert_chain = certs(cert_file).unwrap();

        let mut keys = Vec::new();

        let pems = std::fs::read(key)?;
        for pem in parse_many(pems)? {
            if pem.tag.contains("PRIVATE KEY") {
                keys.push(PrivateKey(pem.contents));
            }
        }

        if keys.len() > 1 {
            anyhow::bail!(
                "TLS configuration error: Found too many keys in the key file - found: {}",
                keys.len()
            );
        }

        if let Some(key) = keys.pop() {
            tls_config
                .set_single_cert(cert_chain, key)
                .context("Failed to set TLS certificate")?;
        } else {
            anyhow::bail!("TLS configuration error: No key found in the key file")
        }

        Ok(tls_config)
    }
}
