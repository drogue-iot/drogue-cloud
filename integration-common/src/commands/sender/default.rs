use super::*;
use async_trait::async_trait;
use reqwest::Method;
use serde_json::json;

pub struct DefaultSender;

#[async_trait]
impl Sender for DefaultSender {
    async fn send(
        &self,
        ctx: Context,
        endpoint: registry::v1::ExternalCommandEndpoint,
        command: CommandOptions,
        payload: web::Bytes,
    ) -> Result<(), Error> {
        let builder = super::to_builder(ctx.client, Method::POST, &endpoint, Ok)?;

        let payload = base64::encode(payload);

        let builder = builder.json(&json!({
            "application": command.application,
            "device": command.device,
            "command": command.command,
            "payload": payload,
        }));

        let resp = builder
            .send()
            .await
            .map_err(|err| Error::Transport(Box::new(err)))?;

        match resp.status() {
            code if code.is_success() => Ok(()),
            _ => Err(super::default_error(resp).await),
        }
    }
}
