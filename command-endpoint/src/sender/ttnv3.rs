use super::*;
use async_trait::async_trait;
use reqwest::Method;
use serde_json::json;

pub struct TtnV3Sender;

#[async_trait]
impl Sender for TtnV3Sender {
    async fn send(
        &self,
        ctx: Context,
        endpoint: registry::v1::ExternalEndpoint,
        command: CommandOptions,
        payload: web::Bytes,
    ) -> Result<(), Error> {
        let device_id = ctx.device_id;
        let builder = super::to_builder(ctx.client, Method::POST, &endpoint, |mut url| {
            url.path_segments_mut()
                .map_err(|_| Error::Payload("Failed to extend path".into()))?
                .extend(&[&device_id, "down", "push"]);
            Ok(url)
        })?;

        let payload = base64::encode(payload);

        let (port, payload) = if let Some(port) = command.command.strip_prefix("port:") {
            // send as raw payload, with selected port number

            let port = port.parse::<u8>().map_err(|err| {
                Error::Payload(format!(
                    "Using 'port:<port>' command, but port was not a valid integer: {}",
                    err
                ))
            })?;

            (port, payload)
        } else {
            // send as JSON payload

            (
                1, // FIXME: need to make this configurable
                serde_json::to_string(&json!({
                  "command": command.command,
                  "command_payload": payload,
                }))
                .map_err(|err| Error::Payload(format!("Failed to encode payload: {}", err)))?,
            )
        };

        let payload = json!({
            "downlinks": [{
                "f_port": port,
                "frm_payload": payload,
            }]
        });

        // send

        log::debug!("Sending payload: {:#?}", payload);

        let resp = builder
            .json(&payload)
            .send()
            .await
            .map_err(|err| Error::Transport(Box::new(err)))?;

        match resp.status() {
            code if code.is_success() => Ok(()),
            _ => Err(super::default_error(resp).await),
        }
    }
}
