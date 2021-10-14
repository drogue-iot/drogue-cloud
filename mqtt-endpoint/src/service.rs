use crate::{auth::DeviceAuthenticator, server::Session, x509::ClientCertificateRetriever};
use async_trait::async_trait;
use bytes::Bytes;
use bytestring::ByteString;
use drogue_cloud_endpoint_common::{
    command::Commands, error::EndpointError, sender::DownstreamSender, sink::Sink,
    x509::ClientCertificateChain,
};
use drogue_cloud_mqtt_common::{
    error::ServerError,
    mqtt::{Connect, Service},
};
use drogue_cloud_service_api::auth::device::authn::Outcome as AuthOutcome;
use drogue_cloud_service_common::Id;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct App<S>
where
    S: Sink,
{
    pub downstream: DownstreamSender<S>,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
}

impl<S> App<S>
where
    S: Sink,
{
    /// authenticate a client
    pub async fn authenticate(
        &self,
        username: Option<&ByteString>,
        password: Option<&Bytes>,
        client_id: &ByteString,
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
impl<Io, S> Service<Io, Session<S>> for App<S>
where
    Io: ClientCertificateRetriever + Sync + Send + Debug + 'static,
    S: Sink,
{
    async fn connect(&self, mut connect: Connect<'_, Io>) -> Result<Session<S>, ServerError> {
        log::info!("new connection: {:?}", connect);

        if !connect.clean_session() {
            return Err(ServerError::UnsupportedOperation);
        }

        let certs = connect.io().client_certs();
        let (username, password) = connect.credentials();

        match self
            .authenticate(username, password, connect.client_id(), certs)
            .await
        {
            Ok(AuthOutcome::Pass {
                application,
                device,
                r#as: _,
            }) => {
                let app_id = application.metadata.name.clone();
                let device_id = device.metadata.name.clone();

                let session = Session::<S>::new(
                    self.downstream.clone(),
                    connect.sink(),
                    application,
                    Id::new(app_id.clone(), device_id.clone()),
                    self.commands.clone(),
                );

                Ok(session)
            }
            Ok(AuthOutcome::Fail) => Err(ServerError::AuthenticationFailed),
            Err(_) => Err(ServerError::AuthenticationFailed),
        }
    }
}
