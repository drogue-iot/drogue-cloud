use crate::telemetry::PublishCommonOptions;
use actix_web::{post, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::{self, DownstreamSender},
    error::{EndpointError, HttpEndpointError},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::{auth::authn, management::Device};
use drogue_ttn::http as ttn;
use serde_json::Value;
use std::collections::HashMap;

#[post("")]
pub async fn publish(
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

fn eval_data_schema(
    model_id: Option<String>,
    device: &Device,
    uplink: &ttn::Uplink,
) -> Option<String> {
    model_id.or_else(|| {
        let function_port = uplink.port.to_string();
        get_spec(device, "lorawan")["ports"][function_port]["data_schema"]
            .as_str()
            .map(|str| str.to_string())
    })
}

fn get_spec<'d>(device: &'d Device, key: &str) -> &'d Value {
    device.spec.get(key).unwrap_or(&Value::Null)
}

#[cfg(test)]
mod test {

    use super::*;
    use chrono::Utc;
    use drogue_ttn::http::Metadata;
    use serde_json::{json, Map, Value};

    #[test]
    fn test_model_mapping() {
        let lorawan_spec = json!({
            "ports": {
             "1": { "data_schema": "mod1",},
             "5": {"data_schema": "mod5",},
            }
        });

        let device = device(Some(lorawan_spec));
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &uplink);

        assert_eq!(model_id, Some(String::from("mod5")));
    }

    #[test]
    fn test_model_no_mapping_1() {
        let device = device(None);
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &uplink);

        assert_eq!(model_id, None);
    }

    #[test]
    fn test_model_no_mapping_2() {
        let device = device(Some(json!({
            "ports": { "1": {"data_schema": "mod1"}}
        })));
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &uplink);

        assert_eq!(model_id, None);
    }

    #[test]
    fn test_model_no_mapping_3() {
        let device = device(Some(json!({
            "ports": { "1": {"no_data_schema": "mod1"}}
        })));
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &uplink);

        assert_eq!(model_id, None);
    }

    fn device(lorawan_spec: Option<Value>) -> Device {
        let mut spec = Map::new();
        if let Some(lorawan_spec) = lorawan_spec {
            spec.insert("lorawan".into(), lorawan_spec);
        }
        Device {
            metadata: Default::default(),
            spec,
            status: Default::default(),
        }
    }

    fn default_uplink(port: u16) -> ttn::Uplink {
        ttn::Uplink {
            app_id: "".to_string(),
            dev_id: "".to_string(),
            hardware_serial: "".to_string(),
            port,
            counter: 0,
            is_retry: false,
            confirmed: false,
            payload_raw: vec![],
            payload_fields: Value::Null,
            metadata: Metadata {
                time: Utc::now(),
                frequency: Some(0.0),
                modulation: Some("".to_string()),
                data_rate: None,
                bit_rate: None,
                coding_rate: Some("".to_string()),
                coordinates: None,
                gateways: vec![],
            },
            downlink_url: None,
        }
    }
}
