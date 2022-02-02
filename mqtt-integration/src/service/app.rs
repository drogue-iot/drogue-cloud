use crate::service::{session::Session, ServiceConfig};
use async_trait::async_trait;
use drogue_client::{openid::OpenIdTokenProvider, registry};
use drogue_cloud_endpoint_common::{sender::UpstreamSender, sink::Sink as SenderSink};
use drogue_cloud_mqtt_common::{
    error::ServerError,
    mqtt::{self, *},
};
use drogue_cloud_service_api::auth::user::{
    authn::{AuthenticationRequest, Outcome},
    UserInformation,
};
use drogue_cloud_service_common::{
    client::UserAuthClient,
    openid::{Authenticator, AuthenticatorError},
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct App<S: SenderSink> {
    pub authenticator: Option<Authenticator>,
    pub user_auth: Option<Arc<UserAuthClient>>,
    pub config: ServiceConfig,
    pub sender: UpstreamSender<S>,
    pub client: reqwest::Client,
    pub registry: registry::v1::Client<Option<OpenIdTokenProvider>>,
}

impl<S> App<S>
where
    S: SenderSink,
{
    /// Authenticate a connection from a connect packet
    async fn authenticate(
        &self,
        connect: &Connect<'_>,
        auth: &Authenticator,
    ) -> Result<UserInformation, anyhow::Error> {
        let user = match (connect.credentials(), &self.user_auth) {
            ((Some(username), Some(password)), Some(user_auth)) => {
                log::debug!("Authenticate with username and password");
                // we have a username and password, and are allowed to test this against SSO
                let username = username.to_string();
                let password = String::from_utf8(password.to_vec())?;

                match user_auth
                    .authenticate_access_token(AuthenticationRequest {
                        user_id: username,
                        access_token: password,
                    })
                    .await?
                    .outcome
                {
                    Outcome::Known(details) => UserInformation::Authenticated(details),
                    Outcome::Unknown => {
                        log::debug!("Unknown API key");
                        return Err(AuthenticatorError::Failed.into());
                    }
                }
            }
            ((Some(username), None), _) => {
                log::debug!("Authenticate with token (username only)");
                // username but no username is treated as a token
                let token = auth.validate_token(&username).await?.into();
                UserInformation::Authenticated(token)
            }
            ((None, Some(password)), _) => {
                log::debug!("Authenticate with token (password only)");
                // password but no username is treated as a token
                let password = String::from_utf8(password.to_vec())?;
                let token = auth.validate_token(&password).await?.into();
                UserInformation::Authenticated(token)
            }
            ((None, None), _) => {
                // anonymous authentication, but using user auth
                log::debug!("Anonymous auth");
                UserInformation::Anonymous
            }
            _ => {
                log::debug!("Unknown authentication method");
                anyhow::bail!("Unknown authentication scheme");
            }
        };

        Ok(user)
    }
}

#[async_trait(?Send)]
impl<S> mqtt::Service<Session<S>> for App<S>
where
    S: SenderSink,
{
    async fn connect<'a>(
        &'a self,
        connect: Connect<'a>,
    ) -> Result<ConnectAck<Session<S>>, ServerError> {
        log::debug!("Processing connect request");

        if !connect.clean_session() {
            return Err(ServerError::UnsupportedOperation);
        }

        let user = if let Some(auth) = &self.authenticator {
            // authenticate
            self.authenticate(&connect, auth)
                .await
                .map_err(|_| ServerError::AuthenticationFailed)?
        } else {
            // we are running without authentication
            UserInformation::Anonymous
        };

        let client_id = connect.client_id().to_string();

        let token = match connect.credentials().1 {
            Some(token) => match String::from_utf8(token.to_vec()) {
                Ok(token_string) => Some(token_string),
                Err(_) => None,
            },
            None => None,
        };

        Ok(ConnectAck {
            session: Session::new(
                self.config.clone(),
                self.user_auth.clone(),
                connect.sink(),
                user,
                client_id,
                self.sender.clone(),
                self.client.clone(),
                self.registry.clone(),
                token,
            ),
            ack: AckOptions {
                wildcard_subscription_available: Some(true),
                shared_subscription_available: Some(true),
                retain_available: Some(false),
                ..Default::default()
            },
        })
    }
}
