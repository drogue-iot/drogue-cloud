use actix_web::dev::ServiceRequest;
use actix_web::Error;

use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::extractors::bearer::{BearerAuth, Config};

use jsonwebtokens::{Algorithm, AlgorithmID, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::value::Value;

use log;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    device_id: String,
    valid_until: u64,
}

const JWT_VERIFY_PUBLIC_KEY_ENV_VAR: &str = "JWT_ECDSA_SIGNING_KEY";

pub async fn validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, Error> {
    let pubkey_path =
        std::env::var(JWT_VERIFY_PUBLIC_KEY_ENV_VAR).expect("JWT_ECDSA_SIGNING_KEY must be set");
    let pubkey = std::fs::read(pubkey_path).unwrap();

    let config = req
        .app_data::<Config>()
        .map(|data| data.clone())
        .unwrap_or_else(Default::default);

    match verify_jwt_signature(credentials.token(), &pubkey) {
        Ok(val) => {
            log::debug!("valid token for {}", req.path());
            let claims: Claims = serde_json::from_value(val)?;
            if verify_jwt_claims(&claims, &req) {
                Ok(req)
            } else {
                log::debug!("JWT is valid but was issued for {}", claims.device_id);
                Err(AuthenticationError::from(config).into())
            }
        }
        Err(e) => {
            log::debug!("{}", e);
            Err(AuthenticationError::from(config).into())
        }
    }
}

fn verify_jwt_claims (claims: &Claims, req: &ServiceRequest) -> bool {
    let path : Vec<&str> = req.uri().path().split("/").collect();

    claims.device_id == path[2]
}

fn verify_jwt_signature(
    token: &str,
    pem_data: &[u8],
) -> Result<Value, jsonwebtokens::error::Error> {
    let alg = Algorithm::new_ecdsa_pem_verifier(AlgorithmID::ES256, pem_data)?;
    let verifier = Verifier::create()
        .leeway(5)
        .build()?;
    verifier.verify(&token, &alg)
}