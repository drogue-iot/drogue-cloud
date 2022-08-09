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
use drogue_cloud_service_common::{
    app::{Startup, StartupExt},
    state::StateController,
};
use futures_util::TryFutureExt;
use lazy_static::lazy_static;
use prometheus::{labels, opts, register_int_gauge, IntGauge};

lazy_static! {
    pub static ref CONNECTIONS_COUNTER: IntGauge = register_int_gauge!(opts!(
        "drogue_connections",
        "Connections",
        labels! {
            "protocol" => "mqtt",
            "type" => "endpoint"
        }
    ))
    .unwrap();
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    let commands = Commands::new();

    // state service

    let (states, runner) = StateController::new(config.state.clone()).await?;

    let app = App {
        config: config.endpoint.clone(),
        downstream: DownstreamSender::new(
            KafkaSink::from_config(
                config.kafka_downstream_config.clone(),
                config.check_kafka_topic_ready,
            )?,
            config.instance.clone(),
            config.endpoint_pool.clone(),
        )?,

        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new(config.auth.clone())
                .await?,
        ),
        commands: commands.clone(),

        states,
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

    let srv = srv.err_into();
    startup.spawn(srv);
    startup.spawn(runner.run());
    startup.check(command_source);

    // exiting

    Ok(())
}
