use crate::{
    telemetry::PublishCommonOptions,
    ttn::{eval_data_schema, get_spec},
};
use actix_web::{post, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::{self, DownstreamSender},
    error::{EndpointError, HttpEndpointError},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::authn;
use drogue_ttn::http as ttn;
use std::collections::HashMap;

#[post("")]
pub async fn publish_v2(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Query(opts): web::Query<PublishCommonOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    cert: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    let uplink: ttn::Uplink = serde_json::from_slice(&body).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    let device_id = uplink.clone().dev_id;

    let (application, device) = match auth
        .authenticate_http(
            opts.application,
            opts.device,
            req.headers().get(http::header::AUTHORIZATION),
            cert.map(|c| c.0),
            Some(device_id.clone()),
        )
        .await
        .map_err(|err| HttpEndpointError(err.into()))?
        .outcome
    {
        authn::Outcome::Fail => return Err(HttpEndpointError(EndpointError::AuthenticationError)),
        authn::Outcome::Pass {
            application,
            device,
        } => (application, device),
    };

    log::info!(
        "Application / Device properties: {:?} / {:?}",
        application,
        device
    );

    // eval model_id from query and function port mapping
    let data_schema = eval_data_schema(opts.data_schema.as_ref().cloned(), &device, &uplink);

    let mut extensions = HashMap::new();
    extensions.insert("lorawanport".into(), uplink.port.to_string());
    extensions.insert("loraretry".into(), uplink.is_retry.to_string());
    extensions.insert("hwaddr".into(), uplink.hardware_serial);

    log::info!("Device ID: {}, Data Schema: {:?}", device_id, data_schema);

    let (body, content_type) = match get_spec(&device, "ttn")["payload"]
        .as_str()
        .unwrap_or_default()
    {
        "raw" => (
            uplink.payload_raw.into(),
            Some(mime::APPLICATION_OCTET_STREAM.to_string()),
        ),
        "fields" => (
            uplink.payload_fields.to_string().into(),
            Some(mime::APPLICATION_JSON.to_string()),
        ),
        _ => {
            // Full payload
            (body, None)
        }
    };

    sender
        .publish_http_default(
            downstream::Publish {
                channel: uplink.port.to_string(),
                app_id: application.metadata.name.clone(),
                device_id,
                options: downstream::PublishOptions {
                    time: Some(uplink.metadata.time),
                    content_type,
                    data_schema,
                    extensions,
                    ..Default::default()
                },
            },
            body,
        )
        .await
}
