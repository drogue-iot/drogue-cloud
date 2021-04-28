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

        let pems = pem::parse_many(buf);
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

#[cfg(feature = "rustls")]
mod rustls {
    use rust_tls::{
        internal::msgs::handshake::DigitallySignedStruct, Certificate, ClientConfig,
        HandshakeSignatureValid, RootCertStore, ServerCertVerified, ServerCertVerifier, TLSError,
    };
    use webpki::DNSNameRef;

    struct NoVerifier;

    impl ServerCertVerifier for NoVerifier {
        fn verify_server_cert(
            &self,
            _roots: &RootCertStore,
            _presented_certs: &[Certificate],
            _dns_name: DNSNameRef,
            _ocsp_response: &[u8],
        ) -> Result<ServerCertVerified, TLSError> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &Certificate,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TLSError> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &Certificate,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TLSError> {
            Ok(HandshakeSignatureValid::assertion())
        }
    }

    pub fn make_insecure(client: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        log::warn!("Disabling TLS verification for client. Do not use this in production!");
        let mut tls = ClientConfig::new();
        tls.dangerous()
            .set_certificate_verifier(std::sync::Arc::new(NoVerifier));

        client.use_preconfigured_tls(tls)
    }
}

#[cfg(feature = "rustls")]
pub use rustls::*;
