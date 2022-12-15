use drogue_client::registry;
use drogue_cloud_endpoint_common::{error::HttpEndpointError, sender::UpstreamSender};
use drogue_cloud_integration_common::{self, commands::CommandOptions};
use drogue_cloud_service_api::webapp::{http::header, web, HttpRequest, HttpResponse};
use drogue_cloud_service_common::{client::CommandRoutingClient};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CommandQuery {
    pub command: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn command(
    sender: web::Data<UpstreamSender>,
    client: web::Data<reqwest::Client>,
    command_routing_client: web::Data<CommandRoutingClient>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<CommandQuery>,
    req: HttpRequest,
    body: web::Bytes,
    registry: web::Data<registry::v1::Client>,
) -> Result<HttpResponse, HttpEndpointError> {
    let (app_name, device_name) = path.into_inner();

    log::debug!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        app_name,
        device_name
    );

    let response = futures::try_join!(
        registry.get_app(&app_name),
        registry.get_device_and_gateways(&app_name, &device_name)
    );

    // TODO handle response and use proper url
    let resp = command_routing_client.get(&app_name, &device_name).await.map_err(|err| HttpEndpointError(err.into()))?;
    let builder = client.request(reqwest::Method::POST, resp.state.endpoint);
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let _res = drogue_cloud_integration_common::commands::sender::send_with_builder(
        builder,
        CommandOptions {
            application: app_name.clone(),
            device: device_name.clone(),
            command: opts.command.clone(),
            content_type,
        },
        body.clone(),
    )
    .await;

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
                CommandOptions {
                    application: app_name,
                    device: device_name,
                    command: opts.command,
                    content_type,
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
