use actix_rt::net::TcpStream;
use drogue_cloud_endpoint_common::{
    psk::{Identity, PskIdentityRetriever},
    x509::{ClientCertificateChain, ClientCertificateRetriever},
};
use std::any::Any;

pub fn from_socket(con: &dyn Any) -> (Option<Identity>, Option<ClientCertificateChain>) {
    log::debug!("Try extracting client cert");

    #[cfg(feature = "openssl")]
    {
        log::debug!("Trying openssl");
        if let Some(con) = con.downcast_ref::<actix_tls::accept::openssl::TlsStream<TcpStream>>() {
            return (con.verified_identity(), con.client_certs());
        }
    }
    #[cfg(feature = "rustls")]
    {
        log::debug!("Trying rustls");
        if let Some(con) = con.downcast_ref::<actix_tls::accept::openssl::TlsStream<TcpStream>>() {
            return (None, con.client_certs());
        }
    }

    log::warn!("No provider to extract certificates from");

    (None, None)
}
