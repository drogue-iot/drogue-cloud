use openid::error::Error;
use openid::{Claims, Client, CompactJson, Discovered};
use url::Url;

pub struct Authenticator {
    pub client: Option<openid::Client>,
}

impl Authenticator {
    pub async fn validate_token(&self, token: String) -> Result<(), actix_web::Error> {
        Ok(())
    }
}
