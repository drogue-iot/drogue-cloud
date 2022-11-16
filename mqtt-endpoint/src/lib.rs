mod auth;
mod config;
mod service;

pub use config::Config;
use drogue_cloud_service_api::auth::device::authn::{PreSharedKeyOutcome, PreSharedKeyResponse};

use crate::{auth::DeviceAuthenticator, service::App};
use drogue_cloud_endpoint_common::{
    command::{Commands, KafkaCommandSource},
    psk::Identity,
    sender::DownstreamSender,
    sink::KafkaSink,
};
use drogue_cloud_mqtt_common::server::build;
use drogue_cloud_service_common::{
    app::{Startup, StartupExt},
    state::StateController, command_routing::CommandRoutingController,
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
    let (command_router, command_runner) = CommandRoutingController::new(config.command_routing.clone()).await?;


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
        command_router,
        disable_psk: config.disable_tls_psk,
    };

    let mut psk_verifier = None;
    if !config.disable_tls_psk {
        let auth = app.authenticator.clone();
        /*
        let (psk_req_tx, psk_req_rx) = ntex::channel::channel();
        let (psk_res_tx, psk_res_rx) = ntex::channel::channel();*/
        psk_verifier = Some(Box::new(
            move |identity: Option<&[u8]>, secret_mut: &mut [u8]| {
                let mut to_copy = 0;
                if let Some(Ok(identity)) = identity.map(|s| core::str::from_utf8(s)) {
                    if let Ok(identity) = Identity::parse(identity) {
                        let auth = auth.clone();
                        let app = identity.application().to_string();
                        let device = identity.device().to_string();

                        // Block this thread waiting for a response.
                        let response = std::thread::spawn(move || {
                            // Run a temporary executor for this request
                            let runner = ntex::rt::System::new("ntex-blocking");
                            runner.block_on(async move { auth.request_psk(app, device).await })
                        })
                        .join();

                        if let Ok(Ok(PreSharedKeyResponse {
                            outcome:
                                PreSharedKeyOutcome::Found {
                                    key,
                                    app: _,
                                    device: _,
                                },
                        })) = response
                        {
                            to_copy = std::cmp::min(key.key.len(), secret_mut.len());
                            secret_mut[..to_copy].copy_from_slice(&key.key[..to_copy]);
                        }
                    }
                }
                Ok(to_copy)
            },
        ))
    }

    let srv = build(config.mqtt.clone(), app, &config, psk_verifier)?.run();

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
