use crate::tls::ClientConfig;
use reqwest::Certificate;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
};

/// Convert the name to an HTTP method.
///
/// If the name is empty, [`None`] is returned. If the method is invalid, and error will be returned.
pub fn to_method(name: &str) -> Result<Option<reqwest::Method>, String> {
    if name.is_empty() {
        Ok(None)
    } else {
        match reqwest::Method::from_str(name) {
            Ok(m) => Ok(Some(m)),
            Err(_) => Err(format!("Invalid HTTP method: {}", name)),
        }
    }
}

fn add_cert<P: AsRef<Path>>(
    mut client: reqwest::ClientBuilder,
    cert: P,
) -> anyhow::Result<reqwest::ClientBuilder> {
    let cert = cert.as_ref();
    log::info!("Adding root certificate: {:?}", cert);
    let mut file = File::open(cert)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let pems = pem::parse_many(buf)?;
    let pems = pems
        .into_iter()
        .map(|pem| Certificate::from_pem(&pem::encode(&pem).into_bytes()).map_err(|err| err.into()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    log::info!("Found {} certificates", pems.len());

    // we need rustls for adding root certificates
    client = client.use_rustls_tls();

    for pem in pems {
        log::info!("Adding root certificate: {:?}", pem);
        client = client.add_root_certificate(pem);
    }

    Ok(client)
}

fn make_insecure(client: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    // previously we had to do a few extras for TLS 1.3 with rustls, but that seems fine now.
    log::warn!("Disabling TLS verification for client. Do not use this in production!");
    client
        // me must use rustls, as openssl doesn't support this
        .use_rustls_tls()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
}

/// Allows us to create clients.
///
/// `reqwest` already has a `ClientBuilder`, however it is unable to be cloned. Also it is not
/// possible to get a `ClientBuilder` from an existing `Client`. So we need to re-create all builders
/// and clients.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientFactory {
    insecure: bool,
    ca_certs: Vec<PathBuf>,
}

impl From<ClientConfig> for ClientFactory {
    fn from(config: ClientConfig) -> Self {
        let mut factory = Self {
            insecure: false,
            ca_certs: vec![],
        };

        if config.tls_insecure {
            factory = factory.make_insecure();
        }

        factory = factory.add_ca_certs(config.certificates());

        factory
    }
}

impl ClientFactory {
    /// Create a new client factory from a default [`ClientConfig`].
    pub fn new() -> Self {
        ClientConfig::default().into()
    }

    fn dedup(&mut self) {
        self.ca_certs.sort_unstable();
        self.ca_certs.dedup();
    }

    pub fn make_insecure(mut self) -> Self {
        self.insecure = true;
        self.dedup();
        self
    }

    pub fn add_ca_cert<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.ca_certs.push(path.into());
        self.dedup();
        self
    }

    pub fn add_ca_certs<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        for path in paths {
            self.ca_certs.push(path.into());
        }
        self
    }

    pub fn new_builder(&self) -> anyhow::Result<reqwest::ClientBuilder> {
        let mut builder = reqwest::ClientBuilder::new();

        for ca in &self.ca_certs {
            builder = add_cert(builder, &ca)?;
        }

        if self.insecure {
            builder = make_insecure(builder);
        }

        Ok(builder)
    }

    pub fn new_client(&self) -> anyhow::Result<reqwest::Client> {
        Ok(self.new_builder()?.build()?)
    }

    /// Alias for `new_client`
    #[inline]
    pub fn build(&self) -> anyhow::Result<reqwest::Client> {
        self.new_client()
    }
}
