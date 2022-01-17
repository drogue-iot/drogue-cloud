mod auth;
mod config;
mod service;

pub use config::Config;

use crate::{auth::DeviceAuthenticator, service::App};
use drogue_cloud_endpoint_common::{
    command::{Commands, KafkaCommandSource},
    sender::DownstreamSender,
    sink::KafkaSink,
};
use drogue_cloud_mqtt_common::server::build;
use drogue_cloud_service_common::health::HealthServer;
use futures::TryFutureExt;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let commands = Commands::new();

    let app = App {
        config: config.endpoint.clone(),
        downstream: DownstreamSender::new(
            KafkaSink::from_config(
                config.kafka_downstream_config.clone(),
                config.check_kafka_topic_ready,
            )?,
            config.instance.clone(),
        )?,

        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new(config.auth.clone())
                .await?,
        ),
        commands: commands.clone(),
    };

    let srv = build(config.mqtt.clone(), app, &config)?.run();

    log::info!("Starting web server");

    // command source

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    // run
    if let Some(health) = config.health {
        // health server
        let health = HealthServer::new(health, vec![Box::new(command_source)]);
        futures::try_join!(health.run_ntex(), srv.err_into(),)?;
    } else {
        futures::try_join!(srv)?;
    }

    // exiting

    Ok(())
}
