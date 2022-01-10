use std::ops::Deref;

/// A wrapper for [`drogue_cloud_endpoint_common::auth::DeviceAuthenticator`].
#[derive(Clone, Debug)]
pub struct DeviceAuthenticator(pub drogue_cloud_endpoint_common::auth::DeviceAuthenticator);

impl Deref for DeviceAuthenticator {
    type Target = drogue_cloud_endpoint_common::auth::DeviceAuthenticator;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An implementation that **does not+* verify, it only accepts the certificates.
///
/// This is required because: 1) we call the authentication service at a later time 2) contacting
/// the authentication service involved another network call, and may block. However, the
/// verifier isn't capable of running asynchronous. So we would block the whole I/O loop of the
/// endpoint.
#[cfg(feature = "rustls")]
pub struct AcceptAllClientCertVerifier;

#[cfg(feature = "rustls")]
impl rust_tls::server::ClientCertVerifier for AcceptAllClientCertVerifier {
    fn client_auth_mandatory(&self) -> Option<bool> {
        Some(false)
    }

    fn client_auth_root_subjects(&self) -> Option<rust_tls::DistinguishedNames> {
        Some(rust_tls::DistinguishedNames::new())
    }

    fn verify_client_cert(
        &self,
        _end_entity: &rust_tls::Certificate,
        _intermediates: &[rust_tls::Certificate],
        _now: std::time::SystemTime,
    ) -> Result<rust_tls::server::ClientCertVerified, rust_tls::Error> {
        Ok(rust_tls::server::ClientCertVerified::assertion())
    }
}
