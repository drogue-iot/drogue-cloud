#![type_length_limit = "6000000"]

mod cloudevents;
mod error;
mod mqtt;
mod server;

use crate::server::{build, build_tls};
use bytes::Bytes;
use bytestring::ByteString;
use drogue_cloud_endpoint_common::auth::DeviceProperties;
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator, Outcome as AuthOutcome},
    downstream::DownstreamSender,
    error::EndpointError,
};
use envconfig::Envconfig;
use serde_json::json;
use std::convert::TryInto;

use ntex::http;
use ntex::web;

use futures::future;

#[derive(Clone, Debug, Envconfig)]
struct Config {
    #[envconfig(from = "DISABLE_TLS", default = "false")]
    pub disable_tls: bool,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
    #[envconfig(from = "BIND_ADDR_MQTT")]
    pub bind_addr_mqtt: Option<String>,
    #[envconfig(from = "BIND_ADDR_HTTP", default = "0.0.0.0:8080")]
    pub bind_addr_http: String,
}

#[derive(Clone, Debug)]
pub struct App {
    pub downstream: DownstreamSender,
    pub authenticator: Option<DeviceAuthenticator>,
}

impl App {
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        _: &ByteString,
    ) -> Result<AuthOutcome, EndpointError> {
        match (&self.authenticator, username, password) {
            (None, ..) => Ok(AuthOutcome::Pass(DeviceProperties(json!({})))),
            (Some(authenticator), Some(username), Some(password)) => {
                authenticator
                    .authenticate(
                        &username,
                        &String::from_utf8(password.to_vec())
                            .map_err(|_| EndpointError::AuthenticationError)?,
                    )
                    .await
            }
            (Some(_), _, _) => Ok(AuthOutcome::Fail),
        }
    }
}

#[web::post("/command-service")]
async fn command_service(req: web::HttpRequest, payload: web::types::Payload) -> http::Response {
    log::debug!("Request: {:?}", req);

    let request_event = cloudevents::request_to_event(&req, payload).await;

    log::info!("Event: {:?}", request_event);
    log::info!("Command: {:?}", request_event.unwrap().data());

    web::HttpResponse::Ok().finish()
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::init_from_env()?;

    // test to see if we can create one, although we don't use it now, we would fail early
    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: match config.enable_auth {
            true => Some(AuthConfig::init_from_env()?.try_into()?),
            false => None,
        },
    };

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app)?
    } else {
        build(addr, builder, app)?
    };

    log::info!("Starting web server");

    let web_server = web::server(|| {
        let app = web::App::new();
        app.service(command_service)
    })
    .bind(config.bind_addr_http)?
    .run();

    future::try_join(builder.workers(1).run(), web_server).await?;

    Ok(())
}
