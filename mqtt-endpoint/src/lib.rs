#![type_length_limit = "6000000"]

mod auth;
mod server;
mod service;
mod x509;

use crate::{
    auth::DeviceAuthenticator,
    server::{build, build_tls},
    service::App,
};
use drogue_cloud_endpoint_common::{
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    sender::DownstreamSender,
    sink::KafkaSink,
};
use drogue_cloud_service_common::health::{HealthServer, HealthServerConfig};
use futures::TryFutureExt;
use serde::Deserialize;
use std::fmt::Debug;

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
