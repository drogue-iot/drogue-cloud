use actix_web::{dev::Payload, error, FromRequest, HttpMessage, HttpRequest};
use drogue_cloud_service_api::webapp as actix_web;
use futures_util::future::{ready, Ready};

/// Retrieve client PSK identities, possibly from a TLS stream.
///
/// This trait can be implemented for underlying transport mechanisms to hand over the client
/// certificates.
///
/// There are default implementations for OpenSSL and RusTLS. Works with ntex and actix.
pub trait PskIdentityRetriever {
    fn verified_identity(&self) -> Option<Identity>;
}

impl PskIdentityRetriever for tokio::net::TcpStream {
    fn verified_identity(&self) -> Option<Identity> {
        // we have no certificates
        None
    }
}

#[cfg(feature = "rustls")]
impl<T> PskIdentityRetriever for tokio_rustls::server::TlsStream<T> {
    fn verified_identity(&self) -> Option<Identity> {
        None
    }
}

#[cfg(feature = "openssl")]
impl<T> PskIdentityRetriever for tokio_openssl::SslStream<T> {
    fn verified_identity(&self) -> Option<Identity> {
        self.ssl()
            .psk_identity()
            .map(|i| core::str::from_utf8(i).ok())
            .flatten()
            .map(|s| s.try_into().ok())
            .flatten()
    }
}

#[cfg(feature = "openssl")]
impl PskIdentityRetriever for tokio_dtls_stream_sink::Session {
    fn verified_identity(&self) -> Option<Identity> {
        self.ssl()
            .map(|s| {
                s.psk_identity()
                    .map(|i| core::str::from_utf8(i).ok())
                    .flatten()
            })
            .flatten()
            .map(|s| s.try_into().ok())
            .flatten()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Identity {
    application: String,
    device: String,
}

impl Identity {
    pub fn parse(s: &str) -> Result<Identity, ()> {
        if let Some((d, a)) = s.split_once("@") {
            Ok(Identity {
                application: a.to_string(),
                device: d.to_string(),
            })
        } else {
            Err(())
        }
    }

    pub fn application(&self) -> &str {
        &self.application
    }

    pub fn device(&self) -> &str {
        &self.device
    }
}

impl TryFrom<&str> for Identity {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::parse(s)
    }
}

impl FromRequest for Identity {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let result = req.extensions().get::<Identity>().cloned();

        ready(result.ok_or_else(|| error::ErrorBadRequest("Missing TLS-PSK identity")))
    }
}

#[cfg(all(feature = "ntex", feature = "openssl"))]
impl PskIdentityRetriever for ntex::io::IoBoxed {
    fn verified_identity(&self) -> Option<Identity> {
        // TODO: Not supported by ntex yet
        None
    }
}
