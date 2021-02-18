use crate::commands::{Command, Commands};
use actix_web::dev::Server;
use actix_web::{middleware, post, web, App, HttpResponse, HttpServer};
use cloudevents_sdk_actix_web::HttpRequestExt;
use envconfig::Envconfig;
use std::convert::TryFrom;
use std::ops::{Deref, DerefMut};

#[derive(Envconfig, Clone, Debug)]
pub struct CommandServerConfig {
    #[envconfig(from = "COMMAND_BIND_ADDR", default = "0.0.0.0:8081")]
    pub bind_addr: String,
    #[envconfig(from = "COMMAND_MAX_PAYLOAD_SIZE", default = "65536")]
    pub max_payload_size: usize,
    #[envconfig(from = "COMMAND_MAX_JSON_SIZE", default = "65536")]
    pub max_json_size: usize,
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
                .data(web::JsonConfig::default().limit(max_json_size))
                .data(commands.clone())
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
    req: web::HttpRequest,
    payload: web::Payload,
    commands: web::Data<Commands>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Req: {:?}", req);

    let mut request_event = req.to_event(payload).await?;
    request_event.set_data(
        "application/json",
        String::from_utf8(body.as_ref().to_vec()).unwrap(),
    );

    match Command::try_from(request_event.clone()) {
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
