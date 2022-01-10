use reqwest::Certificate;
use std::{fs::File, io::Read, path::Path};

const SERVICE_CA_CERT: &str = "/var/run/secrets/kubernetes.io/serviceaccount/service-ca.crt";

pub fn add_service_cert(
    mut client: reqwest::ClientBuilder,
) -> anyhow::Result<reqwest::ClientBuilder> {
    let cert = Path::new(SERVICE_CA_CERT);
    if cert.exists() {
        log::info!("Adding root certificate: {:?}", cert);
        let mut file = File::open(cert)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let pems = pem::parse_many(buf)?;
        let pems = pems
            .into_iter()
            .map(|pem| {
                Certificate::from_pem(&pem::encode(&pem).into_bytes()).map_err(|err| err.into())
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        log::info!("Found {} certificates", pems.len());

        // we need rustls for adding root certificates
        client = client.use_rustls_tls();

        for pem in pems {
            log::info!("Adding root certificate: {:?}", pem);
            client = client.add_root_certificate(pem);
        }
    } else {
        log::info!(
            "Service CA certificate does not exist, skipping! ({:?})",
            cert
        );
    }

    Ok(client)
}

pub fn make_insecure(client: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    // previously we had to do a few extras for TLS 1.3 with rustls, but that seems fine now.
    log::warn!("Disabling TLS verification for client. Do not use this in production!");
    client
        // me must use rustls, as openssl doesn't support this
        .use_rustls_tls()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
}
