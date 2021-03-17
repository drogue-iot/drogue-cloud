mod auth;
mod cli;
mod info;
mod spy;

use crate::auth::OpenIdClient;
use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web::{self, Data},
    App, HttpResponse, HttpServer, Responder,
};
use drogue_cloud_service_common::{
    endpoints::{create_endpoint_source, EndpointSourceType},
    openid::{create_client, Authenticator, AuthenticatorConfig},
    openid_auth,
};
use envconfig::Envconfig;
use openid::biscuit::jwk::JWKSet;
use openid::Configurable;
use serde_json::json;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[derive(Clone, Debug, Envconfig)]
pub struct Config {
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
    #[envconfig(
        from = "KAFKA_BOOTSTRAP_SERVERS",
        default = "kafka-eventing-kafka-bootstrap.knative-eventing.svc:9092"
    )]
    pub kafka_boostrap_servers: String,
    #[envconfig(
        from = "KAFKA_TOPIC",
        default = "knative-messaging-kafka.drogue-iot.iot-channel"
    )]
    pub kafka_topic: String,
}

/// Manually clone the client
///
/// See also: https://github.com/kilork/openid/issues/17
fn clone_client(client: &openid::Client) -> openid::Client {
    let jwks = if let Some(jwks) = &client.jwks {
        let keys = jwks.keys.clone();
        Some(JWKSet { keys })
    } else {
        None
    };

    // The following two lines perform a "clone" without having the "Clone" trait.
    // FIXME: get rid of the two .unwrap calls, wait for the upstream fix
    let json = serde_json::to_value(client.provider.config()).unwrap();
    let provider: openid::Config = serde_json::from_value(json).unwrap();

    openid::Client::new(
        provider.into(),
        client.client_id.clone(),
        client.client_secret.clone(),
        client.redirect_uri.as_ref().cloned(),
        client.http_client.clone(),
        jwks,
    )
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::init_from_env()?;

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;

    // extract required endpoint information
    let endpoints = endpoint_source.eval_endpoints().await?;

    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoint_source: Data<EndpointSourceType> = Data::new(endpoint_source);

    // OpenIdConnect

    let enable_auth = config.enable_auth;

    let openid_client = if enable_auth {
        let config = AuthenticatorConfig::init_from_env()?;
        Some(OpenIdClient {
            client: create_client(&config, endpoints.clone()).await?,
            scopes: config.scopes,
        })
    } else {
        None
    };

    let authenticator = openid_client.as_ref().map(|client| {
        let client = clone_client(&client.client);
        web::Data::new(Authenticator::from_client(client))
    });

    let openid_client = openid_client.map(web::Data::new);

    let bind_addr = config.bind_addr.clone();

    // http server

    HttpServer::new(move || {
        let auth = openid_auth!(req -> req.app_data::<web::Data<Authenticator>>().map(|data|data.get_ref()));

        let app = App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::permissive().supports_credentials())
            .data(web::JsonConfig::default().limit(4096))
            .data(config.clone());

        let app = if let Some(authenticator) = &authenticator {
            app.app_data(authenticator.clone())
        } else {
            app
        };

        let app = if let Some(openid_client) = &openid_client {
            app.app_data(openid_client.clone())
        } else {
            app
        };

        app
            .app_data(endpoint_source.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(Condition::new(enable_auth, auth))
                    .service(info::get_info),
            )
            // everything from here on is unauthenticated or not using the middleware
            .service(spy::stream_events) // this one is special, SSE doesn't support authorization headers
            .service(index)
            .service(cli::login)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::code)
            .service(auth::refresh)
            //fixme : use a different port
            .service(health)
    })
    .bind(bind_addr)?
    .run()
    .await?;

    Ok(())
}
