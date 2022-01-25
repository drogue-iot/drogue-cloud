pub mod authn;
pub mod authz;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserDetails {
    pub user_id: String,
    pub roles: Vec<String>,
}

impl UserDetails {
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|s| s == "drogue-admin")
    }
}

#[derive(Clone, Debug)]
pub enum UserInformation {
    Authenticated(UserDetails),
    Anonymous,
}

static EMPTY_ROLES: Vec<String> = vec![];

impl UserInformation {
    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::Authenticated(details) => Some(&details.user_id),
            Self::Anonymous => None,
        }
    }
    pub fn roles(&self) -> &Vec<String> {
        match self {
            Self::Authenticated(details) => &details.roles,
            Self::Anonymous => &EMPTY_ROLES,
        }
    }
    pub fn is_admin(&self) -> bool {
        match self {
            Self::Authenticated(details) => details.is_admin(),
            Self::Anonymous => false,
        }
    }
}

#[cfg(feature = "actix")]
impl actix_web::FromRequest for UserInformation {
    type Error = actix_web::Error;
    type Future = core::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        use actix_web::HttpMessage;
        match req.extensions().get::<UserInformation>() {
            Some(user) => core::future::ready(Ok(user.clone())),
            None => core::future::ready(Ok(UserInformation::Anonymous)),
        }
    }
}
