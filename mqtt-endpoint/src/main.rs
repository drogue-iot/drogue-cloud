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
    auth::AuthConfig, command_router::Id, downstream::DownstreamSender, error::EndpointError,
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::Outcome as AuthOutcome;
use envconfig::Envconfig;
use futures::future;
use ntex::web;
use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Envconfig)]
pub struct Config {
    #[envconfig(from = "DISABLE_TLS", default = "false")]
    pub disable_tls: bool,
    #[envconfig(from = "CERT_BUNDLE_FILE")]
    pub cert_file: Option<String>,
    #[envconfig(from = "KEY_FILE")]
    pub key_file: Option<String>,
    #[envconfig(from = "BIND_ADDR_MQTT")]
    pub bind_addr_mqtt: Option<String>,
    #[envconfig(from = "BIND_ADDR_HTTP", default = "0.0.0.0:8080")]
    pub bind_addr_http: String,
}

#[derive(Clone, Debug)]
pub struct App {
    pub downstream: DownstreamSender,
    pub authenticator: DeviceAuthenticator,
    pub devices: Arc<Mutex<HashMap<Id, tokio::sync::mpsc::Sender<String>>>>,
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
            .map_err(|_| EndpointError::AuthenticationError)?;

        Ok(self
            .authenticator
            .authenticate_mqtt(username.as_ref(), password, &client_id, certs)
            .await
            .map_err(|err| EndpointError::AuthenticationServiceError {
                source: Box::new(err),
            })?
            .outcome)
    }
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::init_from_env()?;

    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: DeviceAuthenticator(AuthConfig::init_from_env()?.try_into()?),
        devices: Arc::new(Mutex::new(HashMap::new())),
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

    let web_server = web::server(move || {
        web::App::new()
            .data(web_app.clone())
            .service(command_service)
    })
    .bind(config.bind_addr_http)?
    .run();

    future::try_join(builder.workers(1).run(), web_server).await?;

    Ok(())
}
