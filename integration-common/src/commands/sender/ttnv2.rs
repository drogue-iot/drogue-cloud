use super::*;
use async_trait::async_trait;
use reqwest::Method;
use serde_json::json;

pub struct TtnV2Sender;

#[async_trait]
impl Sender for TtnV2Sender {
    async fn send(
        &self,
        ctx: Context,
        endpoint: registry::v1::ExternalEndpoint,
        command: CommandOptions,
        payload: web::Bytes,
    ) -> Result<(), Error> {
        let builder = super::to_builder(ctx.client, Method::POST, &endpoint, Ok)?;

        let payload = base64::encode(payload);

        let builder = if let Some(port) = command.command.strip_prefix("port:") {
            // send as raw payload, with selected port number
            let port = port.parse::<u8>().map_err(|err| {
                Error::Payload(format!(
                    "Using 'port:<port>' command, but port was not a valid integer: {}",
                    err
                ))
            })?;

            builder.json(&json!({
              "dev_id": command.device,
              "port": port,
              "confirmed": false,
              "payload_raw": payload,
            }))
        } else {
            // send as JSON payload

            builder.json(&json!({
              "dev_id": command.device,
              "port": 1, // FIXME: need to make this configurable
              "confirmed": false,
              "payload_fields": {
                "command": command.command,
                "command_payload": payload,
              }
            }))
        };

        // send

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
