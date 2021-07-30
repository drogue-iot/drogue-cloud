use actix_web::{http::header, web, HttpResponse};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use drogue_client::{registry, Context};
use drogue_cloud_endpoint_common::{
    downstream::UpstreamSender, error::HttpEndpointError, sink::Sink,
};
use drogue_cloud_integration_common::{self, commands::CommandOptions};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CommandQuery {
    pub command: String,
}

pub async fn command<S>(
    sender: web::Data<UpstreamSender<S>>,
    client: web::Data<reqwest::Client>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<CommandQuery>,
    req: web::HttpRequest,
    body: web::Bytes,
    registry: web::Data<registry::v1::Client>,
    token: BearerAuth,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: Sink,
{
    let (app_name, device_name) = path.into_inner();

    log::debug!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        app_name,
        device_name
    );

    let response = futures::try_join!(
        registry.get_app(
            &app_name,
            Context {
                provided_token: Some(token.token().into()),
            }
        ),
        registry.get_device_and_gateways(
            &app_name,
            &device_name,
            Context {
                provided_token: Some(token.token().into()),
            },
        )
    );

    match response {
        Ok((Some(application), Some(device_gateways))) => {
            let content_type = req
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            drogue_cloud_integration_common::commands::process_command(
                application,
                device_gateways.0,
                device_gateways.1,
                &sender,
                client.get_ref().clone(),
                content_type,
                CommandOptions {
                    application: app_name,
                    device: device_name,
                    command: opts.command,
                },
                body,
            )
            .await
        }
        Ok(_) => Ok(HttpResponse::NotAcceptable().finish()),
        Err(err) => {
            log::info!("Error {:?}", err);
            Ok(HttpResponse::NotAcceptable().finish())
        }
    }
}
