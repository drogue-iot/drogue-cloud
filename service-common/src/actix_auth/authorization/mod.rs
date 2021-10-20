use crate::client::UserAuthClient;
use crate::error::ServiceError;

use drogue_cloud_service_api::auth::user::authz::{AuthorizationRequest, Permission};
use drogue_cloud_service_api::auth::user::{authz, UserInformation};

mod middleware;

#[derive(Clone)]
pub struct AuthZ {
    pub client: Option<UserAuthClient>,
    pub permission: Permission,
}

impl AuthZ {
    /// Authorise a request
    pub async fn authorize(
        &self,
        application: &str,
        user: UserInformation,
    ) -> Result<(), ServiceError> {
        match &self.client {
            Some(client) => {
                log::debug!(
                    "Authorizing - user: {:?}, app: {}, permission: {:?}",
                    user,
                    application,
                    &self.permission
                );

                let response = client
                    .authorize(
                        AuthorizationRequest {
                            application: application.to_string(),
                            permission: self.permission,
                            user_id: user.user_id().map(ToString::to_string),
                            roles: user.roles().clone(),
                        },
                        Default::default(),
                    )
                    .await
                    .map_err(|e| ServiceError::InternalError(e.to_string()))?;

                log::debug!("Outcome: {:?}", response);

                match response.outcome {
                    authz::Outcome::Allow => Ok(()),
                    authz::Outcome::Deny => {
                        Err(ServiceError::InvalidRequest(String::from("Unauthorized")))
                    }
                }
            }
            // No auth client
            None => Err(ServiceError::InternalError(String::from(
                "Missing Authorization client.",
            ))),
        }
    }
}
