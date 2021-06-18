use actix_web::{http::header, web, HttpResponse};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use drogue_client::{registry, Context};
use drogue_cloud_endpoint_common::{
    downstream::{DownstreamSender, DownstreamSink},
    error::HttpEndpointError,
};
use drogue_cloud_integration_common::{self, commands::CommandOptions};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CommandQuery {
    pub command: String,
}

pub async fn command<S>(
    sender: web::Data<DownstreamSender<S>>,
    client: web::Data<reqwest::Client>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<CommandQuery>,
    req: web::HttpRequest,
    body: web::Bytes,
    registry: web::Data<registry::v1::Client>,
    token: BearerAuth,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: DownstreamSink,
{
    let (application, device) = path.into_inner();

    log::debug!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        application,
        device
    );

    let response = registry
        .get_device_and_gateways(
            &application,
            &device,
            Context {
                provided_token: Some(token.token().into()),
            },
        )
        .await;

    match response {
        Ok(Some(device_gateways)) => {
            let content_type = req
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            drogue_cloud_integration_common::commands::process_command(
                device_gateways.0,
                device_gateways.1,
                &sender,
                client.get_ref().clone(),
                content_type,
                CommandOptions {
                    application,
                    device,
                    command: opts.command,
                },
                body,
            )
            .await
        }
        Ok(None) => Ok(HttpResponse::NotAcceptable().finish()),
        Err(err) => {
            log::info!("Error {:?}", err);
            Ok(HttpResponse::NotAcceptable().finish())
        }
    }
}
