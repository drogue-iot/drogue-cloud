mod mock;

pub use mock::*;

use crate::{
    error::ServiceError,
    openid::{Authenticator, AuthenticatorError},
};
use actix_web::{dev::ServiceRequest, HttpMessage};
use drogue_cloud_service_api::auth::user::UserInformation;
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_api::webapp::extractors::bearer::BearerAuth;

pub async fn openid_validator<F>(
    req: ServiceRequest,
    auth: BearerAuth,
    extract: F,
) -> Result<ServiceRequest, actix_web::Error>
where
    F: Fn(&ServiceRequest) -> Option<&Authenticator>,
{
    let token = auth.token().to_string();

    let authenticator = extract(&req);
    log::debug!("Authenticator: {:?}", authenticator);
    let authenticator = authenticator.ok_or_else(|| {
        log::warn!("OAuth authentication is enabled, but we are missing the authenticator");
        ServiceError::InternalError("Missing authenticator instance".into())
    })?;

    match authenticator.validate_token(token).await {
        Ok(payload) => {
            req.extensions_mut()
                .insert(UserInformation::Authenticated(payload.into()));
            Ok(req)
        }
        Err(AuthenticatorError::Missing) => {
            Err(ServiceError::InternalError("Missing OpenID client".into()).into())
        }
        Err(AuthenticatorError::Failed) => Err(ServiceError::AuthenticationError.into()),
    }
}

#[macro_export]
macro_rules! openid_auth {
    ($req:ident -> $($extract:tt)* ) => {
	actix_web::middleware::Compat::new(drogue_cloud_service_api::webapp::HttpAuthentication::bearer(|req, auth| $crate::auth::openid_validator(req, auth, |$req| $($extract)*)))
    };
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::openid::ExtendedClaims;
    use openid::biscuit::jws::Compact;
    use openid::{Empty, Jws};

    #[test]
    fn test_decode() {
        let token = r#"eyJhbGciOiJSUzI1NiIsInR5cCIgOiAiSldUIiwia2lkIiA6ICJEZ2hoSVVwV2llSU5jX0Jtc0lDckhHbm1WTDNMMTMteURtVmp3N2MwUnlFIn0.eyJleHAiOjE2MTg0OTQ5MjYsImlhdCI6MTYxODQ5NDYyNiwianRpIjoiNjAzYTNhMGYtZTkzMC00ZjE1LTkwMDUtMTZjNzFiMTllNDdiIiwiaXNzIjoiaHR0cHM6Ly9rZXljbG9hay1kcm9ndWUtZGV2LmFwcHMud29uZGVyZnVsLmlvdC1wbGF5Z3JvdW5kLm9yZy9hdXRoL3JlYWxtcy9kcm9ndWUiLCJhdWQiOlsic2VydmljZXMiLCJncmFmYW5hIiwiZGl0dG8iLCJkcm9ndWUiLCJhY2NvdW50Il0sInN1YiI6ImI4ZWZjZjAwLTJmZmYtNDRlYS1hZGU5LWYzNWViMmY0ZmNlMSIsInR5cCI6IkJlYXJlciIsImF6cCI6InNlcnZpY2VzIiwiYWNyIjoiMSIsInJlYWxtX2FjY2VzcyI6eyJyb2xlcyI6WyJvZmZsaW5lX2FjY2VzcyIsInVtYV9hdXRob3JpemF0aW9uIl19LCJyZXNvdXJjZV9hY2Nlc3MiOnsiZ3JhZmFuYSI6eyJyb2xlcyI6WyJncmFmYW5hLWVkaXRvciIsImdyYWZhbmEtYWRtaW4iXX0sImRpdHRvIjp7InJvbGVzIjpbImRpdHRvLXVzZXIiLCJkaXR0by1hZG1pbiJdfSwiZHJvZ3VlIjp7InJvbGVzIjpbImRyb2d1ZS11c2VyIiwiZHJvZ3VlLWFkbWluIl19LCJzZXJ2aWNlcyI6eyJyb2xlcyI6WyJkcm9ndWUtdXNlciIsImRyb2d1ZS1hZG1pbiJdfSwiYWNjb3VudCI6eyJyb2xlcyI6WyJtYW5hZ2UtYWNjb3VudCIsIm1hbmFnZS1hY2NvdW50LWxpbmtzIiwidmlldy1wcm9maWxlIl19fSwic2NvcGUiOiJlbWFpbCBwcm9maWxlIiwiY2xpZW50SWQiOiJzZXJ2aWNlcyIsImVtYWlsX3ZlcmlmaWVkIjpmYWxzZSwiY2xpZW50SG9zdCI6IjE5Mi4xNjguMTIuMSIsInByZWZlcnJlZF91c2VybmFtZSI6InNlcnZpY2UtYWNjb3VudC1zZXJ2aWNlcyIsImNsaWVudEFkZHJlc3MiOiIxOTIuMTY4LjEyLjEifQ.JNvytxz-IqTXXoUKF8xZMw-diS7jtkz9GP4u6MRo9iny410zTxSl5Z_O9Mhy1LofxPBMYt65JWs6tRBdKAEXa0w5bLbZdyRgdr3SJpDAxIz6CezCHqSDl1OSQPrW_rWmaS_9XLWxl8fgADwLCNjWbrZrsls_E_rDdfjqhrvcE4f2__lIV_oeG7zcfyYJzNVoZ3Ukyadxq6fwAMf8kZwU_6R6hClb0Ya6jLpNE3miy3ZgugZ1QLJT3tSTyyxzSHMy8146ncBughepequ-zKSnbzQjhgwQsARjjv7bBeZgRjRY6kF3Wr8JalaR2DZU49RopfegZ-9PWO2AEH2dxe4OfQ"#;
        let token: Compact<ExtendedClaims, Empty> = Jws::new_encoded(token);

        let payload = token.unverified_payload().unwrap();

        println!("Payload: {:#?}", payload);
        let user = UserInformation::Authenticated(payload.into());

        let roles = user.roles();

        println!("Roles: {:?}", roles);
        assert_eq!(
            roles,
            &[
                "offline_access",
                "uma_authorization",
                "drogue-user",
                "drogue-admin",
                "drogue-user",
                "drogue-admin"
            ]
        )
    }
}
