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
