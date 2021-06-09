mod auth;
mod error;
mod response;
mod telemetry;

//mod command;
//mod server;

use crate::auth::DeviceAuthenticator;
use crate::error::CoapEndpointError;
use crate::response::Responder;
use std::collections::LinkedList;
use telemetry::PublishOptions;
//use cloudevents_sdk_coap::CoapRequestExt;
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{downstream::DownstreamSender, error::EndpointError};
use drogue_cloud_service_api::auth::device::authn::Outcome as AuthOutcome;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures;
use http::HeaderValue;
use log;
use std::net::SocketAddr;

use bytes::Bytes;
use bytestring::ByteString;
use coap::Server;
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
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
    pub bind_addr_coap: Option<String>,
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
    // pub commands: Commands,
}

impl App {
    /// authenticate a client
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        auth: &[u8],
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
            .authenticate_coap(
                username.as_ref(),
                password,
                HeaderValue::from_bytes(auth).as_ref().ok(),
            )
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

fn uri_parser(ll: &LinkedList<Vec<u8>>) -> Result<Vec<String>, EndpointError> {
    let linked_list = ll.iter();
    let option_values = Vec::new();
    let version = linked_list
        .next()
        .map(|x| String::from_utf8(*x).unwrap())
        .filter(|x| x.eq("v1"))
        .ok_or_else(|| EndpointError::InvalidRequest {
            details: "incorrect version number".to_string(),
        })?;
    let channel = linked_list
        .next()
        .map(|x| String::from_utf8(*x).unwrap())
        .ok_or_else(|| EndpointError::InvalidRequest {
            details: "error parsing channel".to_string(),
        })?;

    let subject = String::new();
    for i in linked_list {
        subject.push_str(std::str::from_utf8(i).map_err(|err| {
            return EndpointError::InvalidRequest {
                details: "error parsing channel".to_string(),
            };
        })?);
        subject.push('/');
    }

    option_values.push(channel);
    if subject.len() != 0 {
        subject.pop();
        option_values.push(subject);
    }
    Ok(option_values)
}

async fn publish_handler(request: CoapRequest<SocketAddr>, app: App) -> Option<CoapResponse> {
    let path_segments = request
        .message
        .get_option(CoapOption::UriPath)
        .map(uri_parser)?
        .unwrap();
    let queries = request
        .message
        .get_option(CoapOption::UriQuery)
        .map(|x| (x.front().unwrap()));
    let auth = request
        .message
        .get_option(CoapOption::Unknown(4209))
        .map(|x| x.front().unwrap())
        .unwrap();

    match path_segments.len() {
        1 => telemetry::publish_plain(
            app.downstream,
            app.authenticator,
            path_segments[0].clone(),
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x))?
                .unwrap(),
            request.clone(),
            auth,
        )
        .await
        .respond_to(&request),

        2 => telemetry::publish_tail(
            app.downstream,
            app.authenticator,
            (path_segments[0].clone(), path_segments[1].clone()),
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x))?
                .unwrap(),
            request.clone(),
            auth,
        )
        .await
        .respond_to(&request),

        _ => Err(CoapEndpointError(EndpointError::InvalidRequest {
            details: "Invalid Path".to_string(),
        }))
        .respond_to(&request),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;

    let addr = config.bind_addr_coap.as_deref().unwrap_or("127.0.0.1:5683");

    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new().await?,
        ),
    };

    println!("Server up on {}", addr);

    let device_to_endpoint = tokio::spawn(async move {
        let mut server = Server::new(addr).unwrap();
        server
            .run(|request| publish_handler(request, app.clone()))
            .await
            .unwrap();
    });

    let health = HealthServer::new(config.health, vec![]);

    futures::try_join!(health.run_ntex(), device_to_endpoint)?;

    Ok(())
}

/*
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    command_endpoint::CommandServerConfig,//{CommandServer, CommandServerConfig},
    commands::Commands,
    downstream::DownstreamSender,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
//use std::ops::DerefMut;

drogue_cloud_endpoint_common::retriever!();

#[cfg(feature = "rustls")]
drogue_cloud_endpoint_common::retriever_rustls!(actix_tls::connect::ssl::rustls::TlsStream<T>);

#[cfg(feature = "openssl")]
drogue_cloud_endpoint_common::retriever_openssl!(actix_tls::connect::ssl::openssl::SslStream<T>);

#[cfg(feature = "ntex")]
retriever_none!(ntex::rt::net::TcpStream);

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::max_payload_size")]
    pub max_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,

    #[serde(default)]
    pub health: HealthServerConfig,

    #[serde(default)]
    pub command: CommandServerConfig,
}

fn main_2() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new()?;
    let commands = Commands::new();

    let config = Config::from_env()?;
    let max_payload_size = config.max_payload_size;
    let max_json_payload_size = config.max_json_payload_size;
    let http_server_commands = commands.clone();

    let device_authenticator = DeviceAuthenticator::new().await?;

    let http_server = HttpServer::new(move || {
        let app = App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::PayloadConfig::new(max_payload_size))
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone())
            .data(http_server_commands.clone());

        let app = app.app_data(Data::new(device_authenticator.clone()));

        app.service(index)
            // the standard endpoint
            .service(
                web::scope("/v1")
                    .service(telemetry::publish_plain)
                    .service(telemetry::publish_tail),
            )

    });
    //.on_connect(|con, ext| {});

    let http_server = http_server.run();

    //let mut command_server = CommandServer::new(config.command, commands.clone())?;

    // health server

    let health = HealthServer::new(config.health, vec![]);

    futures::try_join!(
        health.run(),
    //    command_server.deref_mut().err_into(),
        http_server.err_into()
    )?;

    Ok(())
}
*/
