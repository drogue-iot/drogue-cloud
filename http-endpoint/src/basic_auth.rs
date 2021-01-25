use actix_web::dev::ServiceRequest;
use actix_web::{Error, HttpMessage};
use actix_web_httpauth::extractors::basic::BasicAuth;
use drogue_cloud_endpoint_common::auth::DeviceAuthDetails;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    error::{EndpointError, HttpEndpointError},
};
use drogue_cloud_service_api::auth::Outcome;

// we might need to url-decode the username
const URLDECODE: bool = true;

pub async fn basic_validator(
    req: ServiceRequest,
    cred: BasicAuth,
) -> Result<ServiceRequest, Error> {
    let authenticator = req.app_data::<DeviceAuthenticator>().ok_or_else(|| {
        HttpEndpointError(EndpointError::ConfigurationError {
            details: "Missing authentication configuration".into(),
        })
    })?;

    let (user_id, password) = match URLDECODE {
        true => (
            percent_encoding::percent_decode_str(cred.user_id()).decode_utf8_lossy(),
            cred.password()
                .map(|password| percent_encoding::percent_decode_str(password).decode_utf8_lossy()),
        ),
        false => (cred.user_id().clone(), cred.password().cloned()),
    };

    match password {
        Some(password) => match authenticator
            .authenticate_simple(&user_id, &password)
            .await
            .map_err(|err| HttpEndpointError(err.into()))?
            .outcome
        {
            Outcome::Pass { tenant, device } => {
                req.extensions_mut()
                    .insert(DeviceAuthDetails { tenant, device });
                Ok(req)
            }
            Outcome::Fail => Err(HttpEndpointError(EndpointError::AuthenticationError).into()),
        },
        None => Err(HttpEndpointError(EndpointError::AuthenticationError).into()),
    }
}
