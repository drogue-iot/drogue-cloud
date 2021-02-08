mod auth;
mod info;
mod spy;

use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web::{self, Data},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_httpauth::middleware::HttpAuthentication;
use drogue_cloud_service_common::{
    endpoints::{create_endpoint_source, EndpointSourceType},
    error::ServiceError,
    openid::{create_client, AuthConfig, Authenticator, AuthenticatorError},
};
use envconfig::Envconfig;
use serde_json::json;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[derive(Debug, Envconfig)]
struct Config {
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
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

    let (client, scopes) = if enable_auth {
        let config: AuthConfig = AuthConfig::init_from_env()?;
        (
            Some(create_client(&config, endpoints).await?),
            config.scopes,
        )
    } else {
        (None, "".into())
    };

    let authenticator = web::Data::new(Authenticator { client, scopes });

    // http server

    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(|req, auth| {
            let token = auth.token().to_string();

            async {
                let authenticator = req.app_data::<web::Data<Authenticator>>();
                log::info!("Authenticator: {:?}", &authenticator);
                let authenticator = authenticator.ok_or_else(|| ServiceError::InternalError {
                    message: "Missing authenticator instance".into(),
                })?;

                // authenticator.validate_token(token).await?;
                // Ok(req)

                match authenticator.validate_token(token).await {
                    Ok(_) => Ok(req),
                    Err(AuthenticatorError::Missing) => Err(ServiceError::InternalError {
                        message: "Missing authenticator".into(),
                    }
                    .into()),
                    Err(AuthenticatorError::Failed) => {
                        Err(ServiceError::AuthenticationError.into())
                    }
                }
            }
        });

        App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::permissive().supports_credentials())
            .data(web::JsonConfig::default().limit(4096))
            .app_data(authenticator.clone())
            .app_data(endpoint_source.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(Condition::new(enable_auth, auth))
                    .service(info::get_info),
            )
            .service(spy::stream_events) // this one is special, SSE doesn't support authorization headers
            .service(index)
            .service(auth::login)
            .service(auth::code)
            .service(auth::refresh)
            //fixme : use a different port
            .service(health)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
