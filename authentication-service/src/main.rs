use actix_web::{get, web, HttpResponse, HttpServer, App};
use std::collections::HashMap;
//use chrono::{DateTime, Utc};

use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;

use jsonwebtokens as jwt;
use jwt::{Algorithm, AlgorithmID, encode};
use jwt::error::Error;

use crypto::digest::Digest;
use crypto::sha2::Sha256;

use std::time::{SystemTime, UNIX_EPOCH, Duration};

#[derive(Clone, Debug)]
struct Secret {
     salt: String,
     hash: String
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
}



#[get("/authenticate")]
async fn authenticate(
    credentials: web::Query<Credentials>,
    data: web::Data<HashMap<String, Secret>>,
) -> Result<HttpResponse, actix_web::Error> {

    println!("credentials recevied  {:?}", credentials);

    println!("Received Authentication request for device: {:?}", credentials.device_id);
    println!("password given: {:?}", credentials.password);

    let auth_result;
    match Arc::try_unwrap(data.into_inner()) {
        Ok(database) => {
            match database.get(&credentials.device_id) {
                Some(secret) => auth_result = verify_password(&credentials.password, &secret),
                None => auth_result = AuthenticationResult::NotFound
            }
        }
        //Todo panic ?    
        Err(e) =>  {
            println!("got  {:?}", e);
            match e.get(&credentials.device_id) {
                Some(secret) => auth_result = verify_password(&credentials.password, &secret),
                None => auth_result = AuthenticationResult::NotFound
            }
        }
    }

    //issue token if auth is successful
    match auth_result {
        AuthenticationResult::Success => {
            let token = get_jwt_token(&credentials.device_id);
            match token {
                Ok(token) => Ok(HttpResponse::Ok().body(format!("token :{}", token))),
                _ => Ok(HttpResponse::InternalServerError().content_type("text/plain").body("error encoding the JWT"))
            }
        } 
        _ => Ok(HttpResponse::Ok().body(format!("auth failed")))
    }
}

//but RSA can be used as well
const TOKEN_SECRET: &str = "somesymmetricSecret";
const TOKEN_EXPIRATION_SECONDS: u64 = 600;

fn get_jwt_token(device_id: &str) -> Result<String, Error> {
    let alg = Algorithm::new_hmac(AlgorithmID::HS256, TOKEN_SECRET)?;
    let header = json!({ "alg": alg.name() });
    let claims = json!({ 
                    "deviceId":  device_id,
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

    println!("stored hash {}", secret.hash);
    hasher.input_str(&computed_hash);
    computed_hash = hasher.result_str();
    
    println!("hashed password : {}", computed_hash);

    if computed_hash.eq(&secret.hash) {
        AuthenticationResult::Success
    } else {
        AuthenticationResult::Failed
    }
}


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let mut database = HashMap::new();

    database.insert(
                    "device1".to_string(), 
                    Secret { 
                        salt:"alongFixedSizeSaltString".to_string(), 
                        hash:"2188dd0b20077359488b272f485d90dc1267f212b2d9e23e46a281161b54ae3f".to_string() 
                        //password : verysecret
                    }
                );

    HttpServer::new(move || {
        App::new()
            .service(authenticate)
            .data(database.clone())
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await

}