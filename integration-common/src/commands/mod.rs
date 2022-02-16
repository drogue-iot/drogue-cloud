mod sender;

use drogue_client::{registry, Translator};
use drogue_cloud_endpoint_common::{
    error::HttpEndpointError,
    sender::{
        IntoPublishId, Publish, PublishOptions, PublishOutcome, Publisher, ToPublishId,
        UpstreamSender,
    },
    sink::Sink,
};
use drogue_cloud_service_api::webapp::HttpResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CommandOptions {
    pub application: String,
    pub device: String,

    pub command: String,
    pub content_type: Option<String>,
}

/// Main entrypoint for processing commands
pub async fn process_command<S>(
    application: registry::v1::Application,
    device: registry::v1::Device,
    gateways: Vec<registry::v1::Device>,
    sender: &UpstreamSender<S>,
    client: reqwest::Client,
    opts: CommandOptions,
    body: bytes::Bytes,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: Sink,
{
    if !device.attribute::<registry::v1::DeviceEnabled>() {
        return Ok(HttpResponse::NotAcceptable().finish());
    }

    let mut targets = vec![device.metadata.name.clone()];

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

        targets.push(gateway.metadata.name.clone());
    }

    // no hits so far ... send to internal

    log::debug!("Processing command internally");

    for target in targets {
        log::debug!("Delivering to: {}", target);
        match sender
            .publish(
                Publish {
                    channel: opts.command.clone(),
                    application: &application,
                    device: opts.device.to_id(),
                    sender: target.into_id(),
                    options: PublishOptions {
                        content_type: opts.content_type.clone(),
                        ..Default::default()
                    },
                },
                body.clone(),
            )
            .await
        {
            Ok(PublishOutcome::Accepted) => {
                // keep going
            }
            Ok(PublishOutcome::Rejected) => {
                return Ok(HttpResponse::NotAcceptable().finish());
            }
            Ok(PublishOutcome::QueueFull) => {
                return Ok(HttpResponse::ServiceUnavailable().finish());
            }
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(err.to_string()));
            }
        }
    }

    Ok(HttpResponse::Accepted().finish())
}
