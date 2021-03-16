use crate::openid::OpenIdTokenProvider;
use drogue_cloud_service_api::auth::{
    AuthenticationClientError, AuthenticationRequest, AuthenticationResponse, ErrorInformation,
};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use url::Url;

/// An authentication client backed by reqwest.
#[derive(Clone, Debug)]
pub struct ReqwestAuthenticatorClient {
    client: Client,
    auth_service_url: Url,
    token_provider: Option<OpenIdTokenProvider>,
}

impl ReqwestAuthenticatorClient {
    /// Create a new client instance.
    pub fn new(client: Client, url: Url, token_provider: Option<OpenIdTokenProvider>) -> Self {
        Self {
            client,
            auth_service_url: url,
            token_provider,
        }
    }

    async fn inject_token(
        &self,
        builder: RequestBuilder,
    ) -> Result<RequestBuilder, AuthenticationClientError<reqwest::Error>> {
        if let Some(provider) = &self.token_provider {
            let token = provider
                .provide_token()
                .await
                .map_err(|err| AuthenticationClientError::Token(Box::new(err)))?;
            Ok(builder.bearer_auth(token.access_token))
        } else {
            Ok(builder)
        }
    }

    pub async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>> {
        let req = self.client.post(self.auth_service_url.clone());
        let req = self.inject_token(req).await?;

        let response: Response = req.json(&request).send().await.map_err(|err| {
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
