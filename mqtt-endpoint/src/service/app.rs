use crate::{
    auth::DeviceAuthenticator,
    config::EndpointConfig,
    service::session::{dialect::DialectBuilder, Session},
};
use async_trait::async_trait;
use drogue_client::{
    registry::v1::{Application, Device, MqttSpec},
    Translator,
};
use drogue_cloud_endpoint_common::{
    command::Commands,
    error::EndpointError,
    psk::{Identity, VerifiedIdentity},
    sender::DownstreamSender,
    x509::{ClientCertificateChain, ClientCertificateRetriever},
};
use drogue_cloud_mqtt_common::{
    error::ServerError,
    mqtt::{AckOptions, Connect, ConnectAck, Service},
};
use drogue_cloud_service_api::{
    auth::device::authn::Outcome as AuthOutcome,
    auth::device::authn::{PreSharedKeyOutcome, PreSharedKeyResponse},
    services::device_state::LastWillTestament,
};
use drogue_cloud_service_common::state::{CreateOptions, CreationOutcome, StateController};
use std::fmt::Debug;
use tracing::instrument;

#[derive(Clone, Debug)]
pub struct App {
    pub config: EndpointConfig,
    pub downstream: DownstreamSender,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
    pub states: StateController,
    pub disable_psk: bool,
}

impl App {
    /// authenticate a client
    #[instrument(skip_all, fields(
        username,
        has_password = password.is_some(),
        client_id,
        has_certs = certs.is_some(),
        has_verified_identity = verified_identity.is_some(),
    ), err)]
    pub async fn authenticate(
        &self,
        username: Option<&str>,
        password: Option<&[u8]>,
        client_id: &str,
        certs: Option<ClientCertificateChain>,
        verified_identity: Option<VerifiedIdentity>,
    ) -> Result<AuthOutcome, EndpointError> {
        let password = password
            .map(|p| String::from_utf8(p.to_vec()))
            .transpose()
            .map_err(|err| {
                log::debug!("Failed to convert password: {}", err);
                EndpointError::AuthenticationError
            })?;

        Ok(self
            .authenticator
            .authenticate_mqtt(username, password, &client_id, certs, verified_identity)
            .await
            .map_err(|err| {
                log::debug!("Failed to call authentication service: {}", err);
                EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                }
            })?
            .outcome)
    }

    #[instrument(
        skip_all,
        fields(
            application = %application.metadata.name,
            device = %device.metadata.name,
        ),
        err(Debug)
    )]
    async fn create_session(
        &self,
        application: Application,
        device: Device,
        connect: &Connect<'_>,
    ) -> Result<Session, ServerError> {
        // eval dialect
        let dialect = match device
            .section::<MqttSpec>()
            .or_else(|| application.section())
        {
            Some(Ok(mqtt)) => mqtt.dialect,
            Some(Err(err)) => {
                let msg = format!(
                    "Unable to parse MQTT spec section. Rejecting connection. Reason: {err}"
                );
                log::warn!("{msg}");
                return Err(ServerError::Configuration(msg));
            }
            None => Default::default(),
        };

        log::debug!("MQTT dialect: {dialect:?}");

        // validate

        let dialect = dialect.create();

        dialect.validate_connect(connect)?;

        // prepare

        let sink = connect.sink();
        let lwt = Self::make_lwt(&connect);
        log::info!("LWT: {lwt:?}");

        // acquire session

        let opts = CreateOptions { lwt };

        let state = match self
            .states
            .create(&application, &device, self.config.state_attempts, opts)
            .await
        {
            CreationOutcome::Created(handle) => handle,
            CreationOutcome::Occupied => {
                return Err(ServerError::StateError("State still occupied".to_string()));
            }
            CreationOutcome::Failed => {
                return Err(ServerError::InternalError(
                    "Failed to contact state service".to_string(),
                ));
            }
        };

        // return

        Ok(Session::new(
            &self.config,
            self.authenticator.clone(),
            self.downstream.clone(),
            sink,
            application,
            dialect,
            device,
            self.commands.clone(),
            *state,
        ))
    }

    fn make_lwt(connect: &Connect<'_>) -> Option<LastWillTestament> {
        match connect {
            Connect::V3(handshake) => match &handshake.packet().last_will {
                Some(lwt) => Some(LastWillTestament {
                    channel: lwt.topic.as_ref().to_string(),
                    payload: lwt.message.to_vec(),
                    content_type: None,
                }),
                None => None,
            },
            Connect::V5(handshake) => match &handshake.packet().last_will {
                Some(lwt) => Some(LastWillTestament {
                    channel: lwt.topic.as_ref().to_string(),
                    payload: lwt.message.to_vec(),
                    content_type: lwt.content_type.as_ref().map(|s| s.as_ref().to_string()),
                }),
                None => None,
            },
        }
    }

    #[instrument(skip(self))]
    async fn lookup_identity(&self, identity: &Identity) -> Option<VerifiedIdentity> {
        if let Ok(PreSharedKeyResponse {
            outcome:
                PreSharedKeyOutcome::Found {
                    key: _,
                    app,
                    device,
                },
        }) = self
            .authenticator
            .request_psk(identity.application(), identity.device())
            .await
        {
            Some(VerifiedIdentity {
                application: app,
                device,
            })
        } else {
            None
        }
    }
}

#[async_trait(?Send)]
impl Service<Session> for App {
    #[instrument]
    async fn connect<'a>(
        &'a self,
        mut connect: Connect<'a>,
    ) -> Result<ConnectAck<Session>, ServerError> {
        log::info!("new connection: {:?}", connect);

        let certs = connect.io().client_certs();
        let verified_identity = if self.disable_psk {
            None
        } else {
            use ntex_tls::PskIdentity;

            let psk_identity = connect.io().query::<PskIdentity>();
            let psk_identity = if let Some(psk_identity) = psk_identity.as_ref() {
                core::str::from_utf8(&psk_identity.0[..])
                    .ok()
                    .map(|i| Identity::parse(i).ok())
                    .flatten()
            } else {
                None
            };

            if let Some(identity) = psk_identity {
                self.lookup_identity(&identity).await
            } else {
                None
            }
        };
        let (username, password) = connect.credentials();

        match self
            .authenticate(
                username.map(|u| u.as_ref()),
                password.map(|p| p.as_ref()),
                connect.client_id().as_ref(),
                certs,
                verified_identity,
            )
            .await
        {
            Ok(AuthOutcome::Pass {
                application,
                device,
                r#as: _,
            }) => {
                let session = self.create_session(application, device, &connect).await?;

                Ok(ConnectAck {
                    session,
                    ack: AckOptions {
                        wildcard_subscription_available: Some(true),
                        shared_subscription_available: Some(false),
                        subscription_identifiers_available: Some(false),
                        session_present: false,
                        ..Default::default()
                    },
                })
            }
            Ok(AuthOutcome::Fail) => Err(ServerError::AuthenticationFailed),
            Err(_) => Err(ServerError::AuthenticationFailed),
        }
    }
}
