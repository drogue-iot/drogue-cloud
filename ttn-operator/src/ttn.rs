use crate::error::ReconcileError;
use reqwest::{RequestBuilder, Response, StatusCode};
use serde_json::{json, Value};
use url::{PathSegmentsMut, Url};

pub struct Client {
    client: reqwest::Client,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub api_key: String,
    pub url: Url,
}

impl Context {
    pub fn inject_token(&self, req: RequestBuilder) -> RequestBuilder {
        req.bearer_auth(&self.api_key)
    }
}

trait TokenInjector {
    fn inject_token(self, ctx: &Context) -> Self;
}

impl TokenInjector for RequestBuilder {
    fn inject_token(self, ctx: &Context) -> Self {
        ctx.inject_token(self)
    }
}

#[derive(Clone, Debug)]
pub enum Owner {
    User(String),
    Organization(String),
}

impl Owner {
    pub fn extend(&self, path: &mut PathSegmentsMut) {
        match self {
            Self::User(user) => path.extend(&["users", &user]),
            Self::Organization(org) => path.extend(&["organizations", &org]),
        };
    }
}

impl Client {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    fn url<F>(ctx: &Context, f: F) -> Result<Url, ReconcileError>
    where
        F: FnOnce(&mut PathSegmentsMut),
    {
        let mut url = ctx.url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| ReconcileError::permanent("Failed to modify path"))?;
            f(&mut path);
        }
        Ok(url)
    }

    pub async fn create_app(
        &self,
        app_id: &str,
        owner: Owner,
        ctx: &Context,
    ) -> Result<(), ReconcileError> {
        let url = Self::url(&ctx, |path| {
            path.extend(&["api", "v3"]);
            owner.extend(path);
            path.extend(&["applications"]);
        })?;

        let create = json!({
            "application": {
                "ids": {
                    "application_id": app_id,
                },
                "name": app_id,
                "attributes": {
                    "drogue-app": app_id,
                }
            },
        });

        log::debug!("New app: {}", create);

        let res = self
            .client
            .post(url)
            .inject_token(&ctx)
            .json(&create)
            .send()
            .await?;

        if res.status().is_success() {
            let payload = res.text().await?;
            log::debug!("Response: {:#?}", payload);
        } else {
            Self::default_error(res.status(), res).await?;
        }

        Ok(())
    }

    async fn default_error<T>(code: StatusCode, res: Response) -> Result<T, ReconcileError> {
        let info = res.text().await.unwrap_or_default();

        match code {
            StatusCode::NOT_IMPLEMENTED => {
                return Err(ReconcileError::Permanent(format!(
                    "Implementation error: {}: {}",
                    code, info
                )))
            }
            code if code.is_server_error() => Err(ReconcileError::Permanent(format!(
                "Request failed: {}: {}",
                code, info
            ))),
            code => Err(ReconcileError::Temporary(format!(
                "Request failed: {}: {}",
                code, info
            ))),
        }
    }

    pub async fn get_app(
        &self,
        app_id: &str,
        ctx: &Context,
    ) -> Result<Option<Value>, ReconcileError> {
        let url = Self::url(&ctx, |path| {
            path.extend(&["api", "v3", "applications", app_id]);
        })?;

        let res = self.client.get(url).inject_token(&ctx).send().await?;

        match res.status() {
            StatusCode::OK => Ok(Some(res.json().await?)),
            StatusCode::NOT_FOUND => Ok(None),
            code => Self::default_error(code, res).await,
        }
    }

    pub async fn delete_app(&self, app_id: &str, ctx: &Context) -> Result<(), ReconcileError> {
        let url = Self::url(&ctx, |path| {
            path.extend(&["api", "v3", "applications", app_id]);
        })?;

        let res = self.client.get(url).inject_token(&ctx).send().await?;

        match res.status() {
            StatusCode::OK | StatusCode::NOT_FOUND => Ok(()),
            code => Self::default_error(code, res).await,
        }
    }

    pub async fn get_webhook(
        &self,
        app_id: &str,
        webhook_id: &str,
        ctx: &Context,
    ) -> Result<Option<Value>, ReconcileError> {
        let mut url = Self::url(&ctx, |path| {
            path.extend(&["api", "v3", "as", "webhooks", app_id, webhook_id]);
        })?;

        {
            url
                .query_pairs_mut()
                .append_pair("field_mask", "headers,uplink_message,join_accept,downlink_ack,downlink_nack,downlink_sent,downlink_failed,downlink_queued,downlink_queue_invalidated,location_solved,service_data");
        }

        let res = self.client.get(url).inject_token(&ctx).send().await?;

        match res.status() {
            StatusCode::OK => Ok(Some(res.json().await?)),
            StatusCode::NOT_FOUND => Ok(None),
            code => Self::default_error(code, res).await,
        }
    }

    pub async fn create_webhook(
        &self,
        app_id: &str,
        webhook_id: &str,
        endpoint_url: &Url,
        auth: &str,
        ctx: &Context,
    ) -> Result<(), ReconcileError> {
        let create = json!({
            "webhook": {
                "ids": {
                    "webhook_id": webhook_id,
                },
                "base_url": endpoint_url,
                "format": "json",
                "uplink_message": {},
                "headers": {
                    "Authorization": auth,
                }
            },
        });

        let mut url = Self::url(&ctx, |path| {
            //path.extend(&["api", "v3", "as", "webhooks", app_id, webhook_id]);
            path.extend(&["api", "v3", "as", "webhooks", app_id]);
        })?;

        /*
                url
                    .query_pairs_mut()
                    .append_pair("field_mask", "headers,uplink_message,join_accept,downlink_ack,downlink_nack,downlink_sent,downlink_failed,downlink_queued,downlink_queue_invalidated,location_solved,service_data");
        */

        log::debug!("New webhook: {}", create);

        let res = self
            .client
            .post(url)
            .inject_token(&ctx)
            .json(&create)
            .send()
            .await?;

        if res.status().is_success() {
            let payload = res.text().await?;
            log::debug!("Response: {:#?}", payload);
        } else {
            Self::default_error(res.status(), res).await?;
        }

        Ok(())
    }
}
