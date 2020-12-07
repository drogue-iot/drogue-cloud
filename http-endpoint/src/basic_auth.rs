use actix_web::dev::ServiceRequest;
use actix_web::{Error, HttpMessage};
use actix_web_httpauth::extractors::basic::BasicAuth;

use drogue_cloud_endpoint_common::auth::Outcome;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    error::{EndpointError, HttpEndpointError},
};

pub async fn basic_validator(
    req: ServiceRequest,
    cred: BasicAuth,
) -> Result<ServiceRequest, Error> {
    let authenticator = req.app_data::<DeviceAuthenticator>().ok_or_else(|| {
        HttpEndpointError(EndpointError::ConfigurationError {
            details: "Missing authentication configuration".into(),
        })
    })?;

    match cred.password() {
        Some(password) => match authenticator
            .authenticate(cred.user_id(), password)
            .await
            .map_err(HttpEndpointError)?
        {
            Outcome::Pass(props) => {
                req.extensions_mut().insert(props);
                Ok(req)
            }
            Outcome::Fail => Err(HttpEndpointError(EndpointError::AuthenticationError).into()),
        },
        None => Err(HttpEndpointError(EndpointError::AuthenticationError).into()),
    }
}
