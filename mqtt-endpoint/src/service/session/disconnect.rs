use drogue_cloud_mqtt_common::{
    error::{PublishError, ServerError},
    mqtt::Disconnect,
};
use ntex_mqtt::v5::codec::DisconnectReasonCode;
use std::{ops::Deref, sync::Arc};
use thiserror::Error;
use tokio::sync::{RwLock, RwLockReadGuard};

#[derive(Clone, Debug, Error)]
pub enum DisconnectError {
    #[error("already disconnected")]
    AlreadyDisconnected,
}

impl From<DisconnectError> for ServerError {
    fn from(err: DisconnectError) -> Self {
        match err {
            DisconnectError::AlreadyDisconnected => ServerError::ProtocolError,
        }
    }
}

impl From<DisconnectError> for PublishError {
    fn from(err: DisconnectError) -> Self {
        match err {
            DisconnectError::AlreadyDisconnected => PublishError::ProtocolError,
        }
    }
}

#[derive(Clone, Copy)]
enum Inner {
    Connected,
    Disconnected { skip_lwt: bool },
    Closed { skip_lwt: bool },
}

impl Inner {
    async fn disconnect(&mut self, skip_lwt: bool) -> Result<(), DisconnectError> {
        match self {
            Self::Connected => {
                *self = Inner::Disconnected { skip_lwt };
                Ok(())
            }
            Self::Disconnected { .. } | Self::Closed { .. } => {
                Err(DisconnectError::AlreadyDisconnected)
            }
        }
    }
}

pub struct DisconnectHandle {
    inner: Arc<RwLock<Inner>>,
}

pub struct ConnectedGuard<'d> {
    _lock: RwLockReadGuard<'d, Inner>,
}

impl DisconnectHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::Connected)),
        }
    }

    /// Ensure that the instance is still connected.
    ///
    /// This will return a lock, which will prevent a disconnect or close as long as the lock is
    /// held.
    pub async fn ensure(&self) -> Result<ConnectedGuard<'_>, DisconnectError> {
        let lock = self.inner.read().await;
        match lock.deref() {
            Inner::Connected => Ok(ConnectedGuard { _lock: lock }),
            Inner::Disconnected { .. } | Inner::Closed { .. } => {
                Err(DisconnectError::AlreadyDisconnected)
            }
        }
    }

    pub async fn disconnect_with(&self, skip_lwt: bool) -> Result<(), DisconnectError> {
        self.inner.write().await.disconnect(skip_lwt).await
    }

    pub async fn disconnected(&self, disconnect: Disconnect<'_>) -> Result<(), DisconnectError> {
        let skip_lwt = match disconnect.reason_code() {
            DisconnectReasonCode::NormalDisconnection => {
                log::debug!("Normal disconnect, skipping LWT");
                true
            }
            _ => false,
        };
        self.disconnect_with(skip_lwt).await
    }

    pub async fn close(&self) -> bool {
        let mut lock = self.inner.write().await;
        match *lock {
            Inner::Connected => {
                *lock = Inner::Closed { skip_lwt: false };
                false
            }
            Inner::Disconnected { skip_lwt } => {
                *lock = Inner::Closed { skip_lwt };
                skip_lwt
            }
            Inner::Closed { skip_lwt } => skip_lwt,
        }
    }
}
