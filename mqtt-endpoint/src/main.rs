#![type_length_limit = "6000000"]

mod auth;
mod cloudevents_sdk_ntex;
mod error;
mod mqtt;
mod server;

use crate::{
    auth::DeviceAuthenticator,
    server::{build, build_tls},
};
use bytes::Bytes;
use bytestring::ByteString;
use cloudevents::event::ExtensionValue;
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    auth::AuthConfig, downstream::DownstreamSender, error::EndpointError,
};
use drogue_cloud_service_api::auth::Outcome as AuthOutcome;
use envconfig::Envconfig;
use futures::future;
use ntex::{http, web};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
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
    pub devices: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<String>>>>,
}

impl App {
    /// authenticate a client
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        client_id: &ByteString,
    ) -> Result<AuthOutcome, EndpointError> {
        match password.as_ref().map(|p| String::from_utf8(p.to_vec())) {
            Some(Ok(password)) => Ok(self
                .authenticator
                .authenticate_mqtt(username.as_ref(), Some(password), &client_id)
                .await
                .map_err(|err| EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                })?
                .outcome),
            _ => Ok(AuthOutcome::Fail),
        }
    }
}

#[web::post("/command-service")]
async fn command_service(
    req: web::HttpRequest,
    payload: web::types::Payload,
    app: web::types::Data<App>,
) -> http::Response {
    log::debug!("Command request: {:?}", req);

    let request_event = cloudevents_sdk_ntex::request_to_event(&req, payload)
        .await
        .unwrap();

    let device_id_ext = request_event.extension("deviceid");

    match device_id_ext {
        Some(ExtensionValue::String(device_id)) => {
            let device = { app.devices.lock().unwrap().get(device_id).cloned() };
            if let Some(sender) = device {
                if let Some(command) = request_event.data() {
                    match sender
                        .send(String::try_from(command.clone()).unwrap())
                        .await
                    {
                        Ok(_) => {
                            log::debug!("Command sent to device {:?}", device_id);
                            web::HttpResponse::Ok().finish()
                        }
                        Err(e) => {
                            log::error!("Failed to send a command {:?}", e);
                            web::HttpResponse::BadRequest().finish()
                        }
                    }
                } else {
                    log::error!("Failed to route command: No command provided!");
                    web::HttpResponse::BadRequest().finish()
                }
            } else {
                log::debug!(
                    "Failed to route command: No device {:?} found on this endpoint!",
                    device_id
                );
                web::HttpResponse::Ok().finish()
            }
        }
        _ => {
            log::error!("Failed to route command: No device provided!");
            web::HttpResponse::BadRequest().finish()
        }
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
