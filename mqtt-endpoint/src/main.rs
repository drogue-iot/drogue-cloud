#![type_length_limit = "6000000"]

mod cloudevents_sdk_ntex;
mod error;
mod mqtt;
mod server;

use crate::server::{build, build_tls};
use bytes::Bytes;
use bytestring::ByteString;
use cloudevents::event::ExtensionValue;
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator},
    downstream::DownstreamSender,
    error::EndpointError,
};
use drogue_cloud_service_api::auth::Outcome as AuthOutcome;
use envconfig::Envconfig;
use futures::future;
use ntex::{http, web};
use std::{
    collections::HashMap,
    convert::TryFrom,
    convert::TryInto,
    sync::{Arc, Mutex},
};

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
    pub devices: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<String>>>>,
}

impl App {
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        _: &ByteString,
    ) -> Result<AuthOutcome, EndpointError> {
        match (
            &self.authenticator,
            username,
            password.as_ref().map(|p| String::from_utf8(p.to_vec())),
        ) {
            (None, ..) => Ok(AuthOutcome::Pass {
                tenant: Default::default(),
                device: Default::default(),
            }),
            (Some(authenticator), Some(username), Some(Ok(password))) => Ok(authenticator
                // FIXME: need to implement other variants as well
                .authenticate_simple(&username, &password)
                .await
                .map_err(|err| EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                })?
                .outcome),
            (Some(_), _, _) => Ok(AuthOutcome::Fail),
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

    let device_id_ext = request_event.extension("device_id");

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

    let config = Config::init_from_env()?;

    // test to see if we can create one, although we don't use it now, we would fail early
    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: match config.enable_auth {
            true => Some(AuthConfig::init_from_env()?.try_into()?),
            false => None,
        },
        devices: Arc::new(Mutex::new(HashMap::new())),
    };

    let web_app = app.clone();

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app)?
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
