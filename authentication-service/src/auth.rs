use jsonwebtokens as jwt;
use jwt::error::Error;
use jwt::{encode, Algorithm, AlgorithmID};

use crypto::sha2::Sha256;
use crypto::digest::Digest;

use serde_json::json;

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::{AuthenticationResult, Secret};

pub(super) fn get_jwt_token(dev_id: &str, pem_data: &[u8], expiration: u64) -> Result<String, Error> {
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

pub(super) fn verify_password(password: &str, secret: Option<String>) -> AuthenticationResult {

    //todo this can probably be done with some 1 liner
    let sec = match secret {
        Some(s) => {
            // turn s into a Secret object
            let sec : Secret = serde_json::from_str(s.as_str()).unwrap();
            sec
        }
        None => return AuthenticationResult::Error
    };

    if password.is_empty() {
       return AuthenticationResult::Error
    }

    let mut computed_hash = password.to_owned() + &sec.salt;
    let mut hasher = Sha256::new();

    hasher.input_str(&computed_hash);
    computed_hash = hasher.result_str();

    if computed_hash.eq(&sec.hash) {
        AuthenticationResult::Success
    } else {
        AuthenticationResult::Failed
    }
}