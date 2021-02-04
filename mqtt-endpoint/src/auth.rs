use rust_tls::{Certificate, DistinguishedNames};
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
pub struct AcceptAllClientCertVerifier;

impl rust_tls::ClientCertVerifier for AcceptAllClientCertVerifier {
    fn client_auth_mandatory(&self, _sni: Option<&webpki::DNSName>) -> Option<bool> {
        Some(false)
    }

    fn client_auth_root_subjects(
        &self,
        _sni: Option<&webpki::DNSName>,
    ) -> Option<DistinguishedNames> {
        Some(DistinguishedNames::new())
    }

    fn verify_client_cert(
        &self,
        _presented_certs: &[Certificate],
        _sni: Option<&webpki::DNSName>,
    ) -> Result<rust_tls::ClientCertVerified, rust_tls::TLSError> {
        Ok(rust_tls::ClientCertVerified::assertion())
    }
}
