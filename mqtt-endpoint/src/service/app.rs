use crate::{auth::DeviceAuthenticator, config::EndpointConfig, service::session::Session};
use async_trait::async_trait;
use drogue_cloud_endpoint_common::{
    command::Commands,
    error::EndpointError,
    sender::DownstreamSender,
    sink::Sink,
    x509::{ClientCertificateChain, ClientCertificateRetriever},
};
use drogue_cloud_mqtt_common::{
    error::ServerError,
    mqtt::{AckOptions, Connect, ConnectAck, Service},
};
use drogue_cloud_service_api::auth::device::authn::Outcome as AuthOutcome;
use std::fmt::Debug;
use tracing::instrument;

#[derive(Clone, Debug)]
pub struct App<S>
where
    S: Sink,
{
    pub config: EndpointConfig,
    pub downstream: DownstreamSender<S>,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
}

impl<S> App<S>
where
    S: Sink,
{
    /// authenticate a client
    #[instrument]
    pub async fn authenticate(
        &self,
        username: Option<&str>,
        password: Option<&[u8]>,
        client_id: &str,
        certs: Option<ClientCertificateChain>,
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
            .authenticate_mqtt(username, password, &client_id, certs)
            .await
            .map_err(|err| {
                log::debug!("Failed to call authentication service: {}", err);
                EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                }
            })?
            .outcome)
    }
}

#[async_trait(?Send)]
impl<S> Service<Session<S>> for App<S>
where
    S: Sink,
{
    #[instrument]
    async fn connect<'a>(
        &'a self,
        mut connect: Connect<'a>,
    ) -> Result<ConnectAck<Session<S>>, ServerError> {
        log::info!("new connection: {:?}", connect);

        if !connect.clean_session() {
            return Err(ServerError::UnsupportedOperation);
        }

        let certs = connect.io().client_certs();
        let (username, password) = connect.credentials();

        match self
            .authenticate(
                username.map(|u| u.as_ref()),
                password.map(|p| p.as_ref()),
                connect.client_id().as_ref(),
                certs,
            )
            .await
        {
            Ok(AuthOutcome::Pass {
                application,
                device,
                r#as: _,
            }) => {
                let session = Session::<S>::new(
                    &self.config,
                    self.authenticator.clone(),
                    self.downstream.clone(),
                    connect.sink(),
                    application,
                    device,
                    self.commands.clone(),
                );

                Ok(ConnectAck {
                    session,
                    ack: AckOptions {
                        wildcard_subscription_available: Some(true),
                        shared_subscription_available: Some(false),
                        subscription_identifiers_available: Some(false),
                        ..Default::default()
                    },
                })
            }
            Ok(AuthOutcome::Fail) => Err(ServerError::AuthenticationFailed),
            Err(_) => Err(ServerError::AuthenticationFailed),
        }
    }
}
