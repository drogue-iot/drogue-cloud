use async_trait::async_trait;
use drogue_cloud_service_api::auth::{
    AuthenticationClient, AuthenticationClientError, AuthenticationRequest, AuthenticationResponse,
    ErrorInformation,
};
use reqwest::{Client, Response, StatusCode};
use url::Url;

/// An authentication client backed by reqwest.
#[derive(Clone, Debug)]
pub struct ReqwestAuthenticatorClient {
    client: Client,
    auth_service_url: Url,
    service_token: Option<openid::Bearer>,
}

impl ReqwestAuthenticatorClient {
    /// Create a new client instance.
    pub fn new(client: Client, url: Url, bearer: Option<openid::Bearer>) -> Self {
        Self {
            client,
            auth_service_url: url,
            service_token: bearer,
        }
    }

    pub fn set_service_token(&mut self, bearer: Option<openid::Bearer>) {
        self.service_token = bearer;
    }
}

#[async_trait]
impl AuthenticationClient for ReqwestAuthenticatorClient {
    type Error = reqwest::Error;

    async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<Self::Error>> {
        let token = match self.clone().service_token {
            Some(t) => t.access_token,
            None => "".to_string(),
        };

        let response: Response = self
            .client
            .post(self.auth_service_url.clone())
            .bearer_auth(token.clone())
            .json(&request)
            .send()
            .await
            .map_err(|err| {
                log::warn!("Error while authenticating {:?}: {}", request, err);
                Box::new(err)
            })?;

        match response.status() {
            StatusCode::OK => match response.json::<AuthenticationResponse>().await {
                Ok(result) => {
                    log::debug!("Outcome for {:?} is {:?}", request, result);
                    Ok(result)
                }
                Err(err) => {
                    log::debug!("Authentication failed for {:?}. Result: {:?}", request, err);

                    Err(AuthenticationClientError::Request(format!(
                        "Failed to decode service response: {}",
                        err
                    )))
                }
            },
            code => match response.json::<ErrorInformation>().await {
                Ok(result) => {
                    log::debug!("Service reported error ({}): {}", code, result);
                    Err(AuthenticationClientError::Service(result))
                }
                Err(err) => {
                    log::debug!(
                        "Authentication failed ({}) for {:?}. Result couldn't be decoded: {:?}",
                        code,
                        request,
                        err
                    );
                    Err(AuthenticationClientError::Request(format!(
                        "Failed to decode service error response: {}",
                        err
                    )))
                }
            },
        }
    }
}
