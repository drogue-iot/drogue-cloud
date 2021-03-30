#![type_length_limit = "6000000"]

mod auth;
mod cloudevents_sdk_ntex;
mod command;
mod error;
mod mqtt;
mod server;
mod x509;

use crate::{
    auth::DeviceAuthenticator,
    command::command_service,
    server::{build, build_tls},
};
use bytes::Bytes;
use bytestring::ByteString;
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    commands::Commands, downstream::DownstreamSender, error::EndpointError,
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::authn::Outcome as AuthOutcome;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures::TryFutureExt;
use ntex::web;
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
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr_http: String,

    #[serde(default)]
    pub health: HealthServerConfig,
}

#[derive(Clone, Debug)]
pub struct App {
    pub downstream: DownstreamSender,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
}

impl App {
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

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;
    let commands = Commands::new();

    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new().await?,
        ),
        commands: commands.clone(),
    };

    let web_app = app.clone();

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app, &config)?
    } else {
        build(addr, builder, app)?
    };

    log::info!("Starting web server");

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // web server

    let web_server = web::server(move || {
        web::App::new()
            .data(web_app.clone())
            .service(command_service)
    })
    .bind(config.bind_addr_http)?
    .run();

    // run

    futures::try_join!(
        health.run_ntex(),
        builder.run().err_into(),
        web_server.err_into(),
    )?;

    // exiting

    Ok(())
}
