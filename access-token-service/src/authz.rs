use async_trait::async_trait;
use drogue_client::user::v1::authz::{Outcome, TokenPermission};
use drogue_cloud_service_api::webapp::http::Method;
use drogue_cloud_service_common::actix_auth::authorization::{Authorizer, Context};
use drogue_cloud_service_common::auth::AuthError;

#[derive(Clone, Debug)]
pub struct TokenOperationAuthorizer;

#[async_trait(?Send)]
impl Authorizer for TokenOperationAuthorizer {
    async fn authorize(&self, context: &Context<'_>) -> Result<Option<Outcome>, AuthError> {
        let outcome = if let Some(claims) = context.identity.token_claims() {
            // Middlewares cannot be registered to routes so we have to determine what type of permission
            // to apply here
            let permission_required = match *context.request.method() {
                Method::GET => TokenPermission::List,
                Method::POST => TokenPermission::Create,
                Method::DELETE => TokenPermission::Delete,
                // this should be unreachable if actix does its job
                _ => {
                    return Err(AuthError::InvalidRequest(
                        "Method not defined in the API. This should never happen.".to_string(),
                    ))
                }
            };

            if claims.tokens.contains(&permission_required) {
                Outcome::Allow
            } else {
                Outcome::Deny
            }
        } else {
            Outcome::Allow
        };
        Ok(Some(outcome))
    }
}
