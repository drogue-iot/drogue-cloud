use actix_web::{get, web, HttpResponse, HttpServer, App};
//use chrono::{DateTime, Utc};

use serde::Deserialize;
use serde_json::json;

use jsonwebtokens as jwt;
use jwt::{Algorithm, AlgorithmID, encode};
use jwt::error::Error;

use crypto::digest::Digest;
use crypto::sha2::Sha256;

use std::time::{SystemTime, UNIX_EPOCH, Duration};

use authentication_service::{PgPool, pg_pool_handler, establish_connection, get_credentials};

#[derive(Clone, Debug, Deserialize)]
struct Secret {
    hash: String,
    salt: String,
     
 }

#[derive(Debug, Deserialize)]
struct Credentials {
    device_id: String,
    password: String
}

enum AuthenticationResult{
    Success,
    NotFound,
    Failed,
    Error,
}


#[get("/authenticate")]
async fn authenticate(
    credentials: web::Query<Credentials>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {

    println!("Received Authentication request for device: {}", credentials.device_id);

    let connection = pg_pool_handler(pool)?;

    let auth_result;
    let db_credentials = get_credentials(&credentials.device_id, &connection);

    if db_credentials.len() > 1 {
        auth_result = AuthenticationResult::Error;
        println!("More than one credential exist for {}", credentials.device_id);
    } else if db_credentials.len() == 1 {
        
        let cred = &db_credentials[0];
        match &cred.secret {
            Some(s) => {
                // turn s into a Secret object
                let secret: Secret = serde_json::from_str(s)?;
                auth_result = verify_password(&credentials.password, &secret);
            },
            None => auth_result = AuthenticationResult::Error,
        }
    } else if db_credentials.len() == 0 {
        auth_result = AuthenticationResult::NotFound;
        println!("No credentials found for {}", credentials.device_id);
    } else {
        auth_result = AuthenticationResult::Error;
    }

    //issue token if auth is successful
    match auth_result {
        AuthenticationResult::Success => {
            let token = get_jwt_token(&credentials.device_id);
            match token {
                Ok(token) => Ok(HttpResponse::Ok().body(format!("token :{}", token))),
                _ => Ok(HttpResponse::InternalServerError().content_type("text/plain").body("error encoding the JWT"))
            }
        },
        AuthenticationResult::Error => Ok(HttpResponse::InternalServerError().content_type("text/plain").body("Internal error")),
        AuthenticationResult::NotFound => Ok(HttpResponse::NotFound().finish()),
        AuthenticationResult::Failed => Ok(HttpResponse::Unauthorized().finish()),
    }
}

//but RSA can be used as well
const TOKEN_SECRET: &str = "somesymmetricSecret";
const TOKEN_EXPIRATION_SECONDS: u64 = 600;

fn get_jwt_token(dev_id: &str) -> Result<String, Error> {
    let alg = Algorithm::new_hmac(AlgorithmID::HS256, TOKEN_SECRET)?;
    let header = json!({ "alg": alg.name() });
    let claims = json!({ 
                    "deviceId":  dev_id,
                    "valid_until": get_future_timestamp(TOKEN_EXPIRATION_SECONDS)
                });

    encode(&header, &claims, &alg)
}


fn get_future_timestamp(seconds_from_now: u64) -> u64 {

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) =>  { 
            match n.checked_add(Duration::new(seconds_from_now, 0)) {
                Some(n) => n.as_secs(),
                _ => 0
            }
        }
        _ => 0
    }
}




//const PASSWORD_HASH_ALGORITHM: str = "SHA256";
fn verify_password(password:  &str, secret: &Secret) -> AuthenticationResult {

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


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let connection_pool = establish_connection();

    HttpServer::new(move || {
        App::new()
            .service(authenticate)
            .data(connection_pool.clone())
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await

}