use actix_web::{
    dev::{Payload, PayloadStream},
    error, FromRequest, HttpRequest,
};
use futures_util::future::{ready, Ready};
use tokio_rustls::rustls::Session;

#[derive(Clone, Debug)]
pub struct ClientCertificateChain(pub Vec<Vec<u8>>);

/// Retrieve client certificates, possibly from a TLS stream.
///
/// This trait can be implemented for underlying transport mechanisms to hand over the client
/// certificates.
///
/// There are default implementations for OpenSSL and RusTLS. Works with ntex and actix.
pub trait ClientCertificateRetriever {
    fn client_certs(&self) -> Option<ClientCertificateChain>;
}

impl ClientCertificateRetriever for tokio::net::TcpStream {
    fn client_certs(&self) -> Option<ClientCertificateChain> {
        // we have no certificates
        None
    }
}

#[cfg(feature = "rustls")]
impl<T> ClientCertificateRetriever for tokio_rustls::server::TlsStream<T> {
    fn client_certs(&self) -> Option<ClientCertificateChain> {
        log::debug!("Try extracting client cert: using rustls");
        self.get_ref()
            .1
            .get_peer_certificates()
            .map(|certs| certs.iter().map(|cert| cert.0.clone()).collect())
            .map(ClientCertificateChain)
    }
}

#[cfg(feature = "openssl")]
impl<T> ClientCertificateRetriever for tokio_openssl::SslStream<T> {
    fn client_certs(&self) -> Option<ClientCertificateChain> {
        log::debug!("Try extracting client cert: using OpenSSL");
        let chain = self.ssl().verified_chain();
        // **NOTE:** This chain (despite the function name) is **NOT** verified.
        // These are the client certificates, which will be passed on to the authentication service.
        let chain = chain
            .map(|chain| {
                log::debug!("Peer cert chain len: {}", chain.len());
                chain
                    .into_iter()
                    .map(|cert| cert.to_der())
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .unwrap_or_else(|err| {
                log::info!("Failed to retrieve client certificate: {}", err);
                None
            });
        log::debug!("Client certificates: {:?}", chain);
        chain.map(ClientCertificateChain)
    }
}

impl FromRequest for ClientCertificateChain {
    type Config = ();
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload<PayloadStream>) -> Self::Future {
        let result = req.extensions().get::<ClientCertificateChain>().cloned();

        ready(result.ok_or_else(|| error::ErrorBadRequest("Missing certificate chain")))
    }
}
