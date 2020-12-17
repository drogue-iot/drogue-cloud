mod auth;

use drogue_cloud_database_common::database;
use drogue_cloud_database_common::models::Secret;
use drogue_cloud_endpoint_common::auth::{AuthRequest, AuthResponse, DeviceProperties, Outcome};

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::extractors::basic::BasicAuth;

use serde_json::json;

use dotenv::dotenv;
use envconfig::Envconfig;

use std::borrow::Cow;

#[derive(Debug)]
enum AuthenticationResult {
    Success,
    Failed,
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[post("/auth")]
async fn password_authentication(
    req: web::Json<AuthRequest>,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("AuthRequest - username: {}", req.username);

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    let cred = match database::get_credential(&req.username, &connection)? {
        Some(cred) => cred,
        None => {
            return Ok(HttpResponse::Ok().json(AuthResponse {
                outcome: Outcome::Fail,
            }));
        }
    };

    let auth_result = auth::verify_password(&req.password, cred.secret);

    Ok(match auth_result {
        Ok(AuthenticationResult::Success) => HttpResponse::Ok().json(AuthResponse {
            outcome: Outcome::Pass(DeviceProperties(
                cred.properties.unwrap_or_else(|| json!({})),
            )),
        }),
        Ok(AuthenticationResult::Failed) => HttpResponse::Ok().json(AuthResponse {
            outcome: Outcome::Fail,
        }),
        Err(_) => HttpResponse::BadRequest().finish(),
    })
}

#[get("/jwt")]
async fn token_authentication(
    auth: BasicAuth,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!(
        "Received Authentication request for device: {}",
        auth.user_id()
    );

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    let cred = match database::get_credential(&auth.user_id(), &connection)? {
        Some(cred) => cred,
        None => {
            return Ok(HttpResponse::Unauthorized().finish());
        }
    };

    let auth_result =
        auth::verify_password(&auth.password().unwrap_or(&Cow::from("")), cred.secret);

    //issue token if auth is successful
    Ok(match auth_result {
        Ok(AuthenticationResult::Success) => {
            let token = auth::get_jwt_token(
                &auth.user_id(),
                &data.token_signing_private_key,
                data.token_expiration_seconds,
            );
            match token {
                Ok(token) => {
                    log::debug!("Issued JWT for device {}. Token: {}", auth.user_id(), token);
                    HttpResponse::Ok()
                        .header("Authorization", token)
                        .json(cred.properties)
                }
                Err(e) => {
                    log::error!("Could not issue JWT token: {}", e);
                    HttpResponse::InternalServerError()
                        .content_type("text/plain")
                        .body("error encoding the JWT")
                }
            }
        }
        Ok(AuthenticationResult::Failed) => HttpResponse::Unauthorized().finish(),
        Err(_) => HttpResponse::BadRequest().finish(),
    })
}

#[derive(Clone)]
struct WebData {
    connection_pool: database::PgPool,
    token_expiration_seconds: u64,
    token_signing_private_key: Vec<u8>,
}

#[derive(Clone, Envconfig)]
struct Config {
    #[envconfig(from = "DATABASE_URL")]
    pub db_url: String,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,

    #[envconfig(from = "TOKEN_EXPIRATION", default = "300")]
    pub jwt_expiration: u64,
    #[envconfig(from = "JWT_ECDSA_SIGNING_KEY")]
    pub jwt_signing_key: Option<String>,
    #[envconfig(from = "ENABLE_JWT", default = "false")]
    pub enable_jwt: bool,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::init_from_env().unwrap();
    let data: WebData;

    let pool = database::establish_connection(config.db_url).expect("Failed to create pool");
    if config.enable_jwt {
        data = WebData {
            connection_pool: pool,
            token_expiration_seconds: config.jwt_expiration,
            token_signing_private_key: std::fs::read(
                config
                    .jwt_signing_key
                    .expect("JWT_ECDSA_SIGNING_KEY must be set"),
            )
            .unwrap(),
        };
    } else {
        data = WebData {
            connection_pool: pool,
            token_expiration_seconds: 0,
            token_signing_private_key: Vec::new(),
        };
    }

    let enable_jwt = config.enable_jwt;
    let max_json_payload_size = config.max_json_payload_size;

    HttpServer::new(move || {
        App::new()
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .service({
                let scope = web::scope("/api/v1").service(password_authentication);

                if enable_jwt {
                    scope.service(token_authentication)
                } else {
                    scope
                }
            })
            //fixme : bind to a different port
            .service(health)
            .data(data.clone())
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
