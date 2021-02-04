use crate::ClientCertificateRetriever;
use actix_rt::net::TcpStream;
use drogue_cloud_endpoint_common::x509::ClientCertificateChain;
use std::any::Any;

pub fn from_socket(con: &dyn Any) -> Option<ClientCertificateChain> {
    log::debug!("Try extracting client cert");

    #[cfg(feature = "openssl")]
    if let Some(con) = con.downcast_ref::<actix_tls::openssl::SslStream<TcpStream>>() {
        return con.client_certs();
    }
    #[cfg(feature = "rustls")]
    if let Some(con) = con.downcast_ref::<actix_tls::rustls::TlsStream<TcpStream>>() {
        return con.client_certs();
    }

    None
}
