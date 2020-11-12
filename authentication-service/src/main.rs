pub mod models;
pub mod schema;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};

use crate::models::Credential;

use actix_web::{get, web, App, HttpResponse, HttpServer};

use serde::Deserialize;
use serde_json::json;

use dotenv::dotenv;

use jsonwebtokens as jwt;
use jwt::error::Error;
use jwt::{encode, Algorithm, AlgorithmID};

use crypto::digest::Digest;
use crypto::sha2::Sha256;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize)]
struct Secret {
    hash: String,
    salt: String,
}

#[derive(Debug, Deserialize)]
struct Credentials {
    device_id: String,
    password: String,
}

#[derive(Debug)]
enum AuthenticationResult {
    Success,
    NotFound,
    Failed,
    Error,
}

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn establish_connection() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .build(manager)
        .expect("Failed to create pool.")
}

pub fn pg_pool_handler(pool: &PgPool) -> Result<PgPooledConnection, HttpResponse> {
    pool.get()
        .map_err(|e| HttpResponse::InternalServerError().json(e.to_string()))
}

pub fn get_credentials(id: &str, pool: &PgConnection) -> Vec<Credential> {
    let results = schema::credentials::dsl::credentials
        .filter(schema::credentials::dsl::device_id.eq(id))
        .load::<Credential>(pool)
        .expect("Error loading credentials");

    results
}

#[get("/authenticate")]
async fn authenticate(
    credentials: web::Query<Credentials>,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!(
        "Received Authentication request for device: {}",
        credentials.device_id
    );

    let connection = pg_pool_handler(&data.connection_pool)?;

    let auth_result;
    let db_credentials = get_credentials(&credentials.device_id, &connection);

    if db_credentials.len() > 1 {
        auth_result = AuthenticationResult::Error;
        log::info!(
            "More than one credential exist for {}",
            credentials.device_id
        );
    } else if db_credentials.len() == 1 {
        let cred = &db_credentials[0];
        match &cred.secret {
            Some(s) => {
                // turn s into a Secret object
                let secret: Secret = serde_json::from_str(s)?;
                auth_result = verify_password(&credentials.password, &secret);
            }
            None => auth_result = AuthenticationResult::Error,
        }
    } else if db_credentials.len() == 0 {
        auth_result = AuthenticationResult::NotFound;
        log::info!("No credentials found for {}", credentials.device_id);
    } else {
        auth_result = AuthenticationResult::Error;
    }

    //issue token if auth is successful
    match auth_result {
        AuthenticationResult::Success => {
            let token = get_jwt_token(
                &credentials.device_id,
                &data.token_signing_private_key,
                data.token_expiration_seconds,
            );
            match token {
                Ok(token) => {
                    log::debug!("Issued JWT for device {}. Token: {}",
                        credentials.device_id,
                        token);
                    Ok(HttpResponse::Ok().header("Authorization", token).finish())
                },
                Err(e) => {
                    log::error!("Could not issue JWT token: {}", e);
                    Ok(HttpResponse::InternalServerError()
                        .content_type("text/plain")
                        .body("error encoding the JWT"))
                }
            }
        }
        AuthenticationResult::Error => Ok(HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body("Internal error")),
        AuthenticationResult::NotFound => Ok(HttpResponse::NotFound().finish()),
        AuthenticationResult::Failed => Ok(HttpResponse::Unauthorized().finish()),
    }
}

fn get_jwt_token(dev_id: &str, pem_data: &[u8], expiration: u64) -> Result<String, Error> {
    let alg = Algorithm::new_ecdsa_pem_signer(AlgorithmID::ES256, pem_data)?;
    let header = json!({ "alg": alg.name() });
    let claims = json!({
        "device_id":  dev_id,
        "exp": get_future_timestamp(expiration)
    });

    encode(&header, &claims, &alg)
}

fn get_future_timestamp(seconds_from_now: u64) -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) => match n.checked_add(Duration::new(seconds_from_now, 0)) {
            Some(n) => n.as_secs(),
            _ => 0,
        },
        _ => 0,
    }
}

fn verify_password(password: &str, secret: &Secret) -> AuthenticationResult {
    let mut computed_hash = password.to_owned() + &secret.salt;
    let mut hasher = Sha256::new();

    hasher.input_str(&computed_hash);
    computed_hash = hasher.result_str();

    if computed_hash.eq(&secret.hash) {
        AuthenticationResult::Success
    } else {
        AuthenticationResult::Failed
    }
}

#[derive(Clone)]
struct WebData {
    connection_pool: PgPool,
    token_expiration_seconds: u64,
    token_signing_private_key: Vec<u8>,
}

const TOKEN_EXPIRATION_SECONDS_ENV_VAR: &str = "TOKEN_EXPIRATION";
const JWT_SIGNING_PRIVATE_KEY_ENV_VAR: &str = "JWT_ECDSA_SIGNING_KEY";

const DEFAULT_TOKEN_EXPIRATION: &str = "300";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    dotenv().ok();

    let pool = establish_connection();
    let jwt_expiration = std::env::var(TOKEN_EXPIRATION_SECONDS_ENV_VAR)
        .unwrap_or(DEFAULT_TOKEN_EXPIRATION.to_string());
    let pem_data = std::fs::read(
        std::env::var(JWT_SIGNING_PRIVATE_KEY_ENV_VAR)
            .expect("JWT_ECDSA_SIGNING_KEY must be set")).unwrap();

    let data = WebData {
        connection_pool: pool,
        token_expiration_seconds: jwt_expiration.parse::<u64>().unwrap(),
        token_signing_private_key: pem_data,
    };

    HttpServer::new(move || App::new().service(authenticate).data(data.clone()))
        .bind("127.0.0.1:8081")?
        .run()
        .await
}