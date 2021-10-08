#![type_length_limit = "6000000"]

mod auth;
mod error;
mod mqtt;
mod server;
mod x509;

use crate::{
    auth::DeviceAuthenticator,
    server::{build, build_tls},
};
use bytes::Bytes;
use bytestring::ByteString;
use drogue_cloud_endpoint_common::{
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    error::EndpointError,
    sender::DownstreamSender,
    sink::{KafkaSink, Sink},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::device::authn::Outcome as AuthOutcome;
use drogue_cloud_service_common::health::{HealthServer, HealthServerConfig};
use futures::TryFutureExt;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    #[serde(default)]
    pub bind_addr_mqtt: Option<String>,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub command_source_kafka: KafkaCommandSourceConfig,
}

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
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        client_id: &ByteString,
        certs: Option<ClientCertificateChain>,
    ) -> Result<AuthOutcome, EndpointError> {
        let password = password
            .as_ref()
            .map(|p| String::from_utf8(p.to_vec()))
            .transpose()
            .map_err(|err| {
                log::debug!("Failed to convert password: {}", err);
                EndpointError::AuthenticationError
            })?;

        Ok(self
            .authenticator
            .authenticate_mqtt(username.as_ref(), password, &client_id, certs)
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

pub async fn run(config: Config) -> anyhow::Result<()> {
    let commands = Commands::new();

    let app = App {
        downstream: DownstreamSender::new(KafkaSink::new("DOWNSTREAM_KAFKA_SINK")?)?,
        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new().await?,
        ),
        commands: commands.clone(),
    };

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app.clone(), &config)?
    } else {
        build(addr, builder, app.clone())?
    };

    log::info!("Starting web server");

    // command source

    let command_source = KafkaCommandSource::new(commands, config.command_source_kafka)?;

    // run
    if let Some(health) = config.health {
        // health server
        let health = HealthServer::new(health, vec![Box::new(command_source)]);
        futures::try_join!(health.run_ntex(), builder.run().err_into(),)?;
    } else {
        futures::try_join!(builder.run())?;
    }

    // exiting

    Ok(())
}
