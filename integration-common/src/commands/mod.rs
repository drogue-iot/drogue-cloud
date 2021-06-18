mod sender;

use actix_web::HttpResponse;
use drogue_client::{registry, Translator};
use drogue_cloud_endpoint_common::{
    downstream::{self, DownstreamSender, DownstreamSink},
    error::HttpEndpointError,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CommandOptions {
    pub application: String,
    pub device: String,

    pub command: String,
}

pub async fn process_command<S>(
    device: registry::v1::Device,
    gateways: Vec<registry::v1::Device>,
    sender: &DownstreamSender<S>,
    client: reqwest::Client,
    content_type: Option<String>,
    opts: CommandOptions,
    body: bytes::Bytes,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: DownstreamSink,
{
    if !device.attribute::<registry::v1::DeviceEnabled>() {
        return Ok(HttpResponse::NotAcceptable().finish());
    }

    for gateway in gateways {
        if !gateway.attribute::<registry::v1::DeviceEnabled>() {
            continue;
        }

        if let Some(command) = gateway.attribute::<registry::v1::Commands>().pop() {
            return match command {
                registry::v1::Command::External(endpoint) => {
                    log::debug!("Sending to external command endpoint {:?}", endpoint);

                    let ctx = sender::Context {
                        device_id: device.metadata.name,
                        client,
                    };

                    match sender::send_to_external(ctx, endpoint, opts, body).await {
                        Ok(_) => Ok(HttpResponse::Ok().finish()),
                        Err(err) => {
                            log::info!("Failed to process external command: {}", err);
                            Ok(HttpResponse::NotAcceptable().finish())
                        }
                    }
                }
            };
        }
    }
    // no hits so far
    sender
        .publish_http_default(
            downstream::Publish {
                channel: opts.command,
                app_id: opts.application,
                device_id: opts.device,
                options: downstream::PublishOptions {
                    topic: None,
                    content_type,
                    ..Default::default()
                },
            },
            body,
        )
        .await
}
