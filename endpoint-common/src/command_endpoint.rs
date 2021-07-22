use crate::commands::{Command, Commands};
use actix_web::{dev::Server, middleware, post, web, App, HttpResponse, HttpServer};
use drogue_cloud_service_common::defaults;
use serde::Deserialize;
use std::convert::TryFrom;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug, Deserialize)]
pub struct CommandServerConfig {
    #[serde(default = "bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::max_payload_size")]
    pub max_payload_size: usize,
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_size: usize,
}

impl Default for CommandServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: bind_addr(),
            max_payload_size: defaults::max_payload_size(),
            max_json_size: defaults::max_json_payload_size(),
        }
    }
}

#[inline]
fn bind_addr() -> String {
    "0.0.0.0:8081".into()
}

pub struct CommandServer {
    server: Server,
}

impl CommandServer {
    pub fn new(
        config: CommandServerConfig,
        commands: Commands,
    ) -> Result<CommandServer, std::io::Error> {
        let max_payload_size = config.max_payload_size;
        let max_json_size = config.max_json_size;

        let server = HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .app_data(web::PayloadConfig::new(max_payload_size))
                .app_data(web::Data::new(
                    web::JsonConfig::default().limit(max_json_size),
                ))
                .app_data(web::Data::new(commands.clone()))
                .service(command_service)
        })
        .bind(config.bind_addr)?
        .run();

        Ok(CommandServer { server })
    }
}

impl TryFrom<CommandServerConfig> for CommandServer {
    type Error = std::io::Error;

    fn try_from(value: CommandServerConfig) -> Result<Self, Self::Error> {
        CommandServer::new(value, Commands::new())
    }
}

impl Deref for CommandServer {
    type Target = Server;

    fn deref(&self) -> &Self::Target {
        &self.server
    }
}

impl DerefMut for CommandServer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.server
    }
}

#[post("/command-service")]
pub async fn command_service(
    body: web::Bytes,
    event: cloudevents::Event,
    commands: web::Data<Commands>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Event: {:?}", event);

    let mut request_event = event.clone();
    request_event.set_data(
        "application/json",
        String::from_utf8(body.as_ref().to_vec()).unwrap(),
    );

    match Command::try_from(request_event) {
        Ok(command) => {
            if let Err(e) = commands.send(command).await {
                log::error!("Failed to route command: {}", e);
                HttpResponse::BadRequest().await
            } else {
                HttpResponse::Ok().await
            }
        }
        Err(_) => {
            log::error!("No device-id provided");
            HttpResponse::BadRequest().await
        }
    }
}
