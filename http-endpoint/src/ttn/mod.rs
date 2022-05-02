mod v2;
mod v3;

pub use v2::*;
pub use v3::*;

use crate::telemetry::PublishCommonOptions;
use chrono::{DateTime, Utc};
use drogue_client::registry;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    error::{EndpointError, HttpEndpointError},
    sender::{self, DownstreamSender, PublishId, PublishIdPair, Publisher},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::{
    auth::device::authn,
    webapp::{web, HttpRequest, HttpResponse},
};
use serde_json::Value;
use std::collections::HashMap;

fn eval_data_schema<S: AsRef<str>>(
    model_id: Option<String>,
    device: &registry::v1::Device,
    r#as: &Option<registry::v1::Device>,
    port: S,
) -> Option<String> {
    model_id.or_else(|| {
        get_spec(device, r#as, "lorawan")["ports"][port.as_ref()]["data_schema"]
            .as_str()
            .map(|str| str.to_string())
    })
}

fn get_spec<'d>(
    device: &'d registry::v1::Device,
    r#as: &'d Option<registry::v1::Device>,
    key: &str,
) -> &'d Value {
    let device = r#as.as_ref().unwrap_or(device);
    device.spec.get(key).unwrap_or(&Value::Null)
}

pub struct Uplink {
    pub device_id: String,
    pub port: String,
    pub time: DateTime<Utc>,
    pub is_retry: Option<bool>,
    pub hardware_address: String,

    pub payload_raw: Vec<u8>,
    pub payload_fields: Value,
}

async fn publish_uplink(
    downstream: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    opts: PublishCommonOptions,
    req: HttpRequest,
    cert: Option<ClientCertificateChain>,
    body: web::Bytes,
    uplink: Uplink,
) -> Result<HttpResponse, HttpEndpointError> {
    let device_id = uplink.device_id;

    let (application, device, r#as) = match auth
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
            r#as,
        } => (application, device, r#as),
    };

    log::info!(
        "Application / Device / Device(as): {:?} / {:?} / {:?}",
        application,
        device,
        r#as,
    );

    // eval model_id from query and function port mapping
    let data_schema = eval_data_schema(
        opts.data_schema.as_ref().cloned(),
        &device,
        &r#as,
        &uplink.port,
    );

    let mut extensions = HashMap::new();
    extensions.insert("lorawanport".into(), uplink.port.clone());
    if let Some(is_retry) = uplink.is_retry {
        extensions.insert("lorawanretry".into(), is_retry.to_string());
    }
    extensions.insert("hwaddr".into(), uplink.hardware_address);

    log::info!("Device ID: {}, Data Schema: {:?}", device_id, data_schema);

    let port = uplink.port.to_string();
    let time = uplink.time;

    let (body, content_type) = match get_spec(&device, &r#as, "ttn")["payload"]
        .as_str()
        .unwrap_or_default()
    {
        "raw" => (
            web::Bytes::from(uplink.payload_raw),
            Some(mime::APPLICATION_OCTET_STREAM.to_string()),
        ),
        "fields" => (
            web::Bytes::from(uplink.payload_fields.to_string()),
            Some(mime::APPLICATION_JSON.to_string()),
        ),
        _ => {
            // Full payload
            (body, None)
        }
    };

    let PublishIdPair { device, sender } = PublishIdPair::with_devices(device, r#as);

    send_uplink(
        downstream,
        application,
        device,
        sender,
        port,
        time,
        content_type,
        data_schema,
        extensions,
        body,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[inline]
async fn send_uplink<B>(
    downstream: web::Data<DownstreamSender>,
    application: registry::v1::Application,
    device: PublishId,
    sender: PublishId,
    port: String,
    time: DateTime<Utc>,
    content_type: Option<String>,
    data_schema: Option<String>,
    extensions: HashMap<String, String>,
    body: B,
) -> Result<HttpResponse, HttpEndpointError>
where
    B: AsRef<[u8]> + Send + Sync,
{
    Ok(downstream
        .publish_http_default(
            sender::Publish {
                channel: port,
                application: &application,
                device,
                sender,
                options: sender::PublishOptions {
                    time: Some(time),
                    content_type,
                    data_schema,
                    extensions,
                    ..Default::default()
                },
            },
            body,
        )
        .await)
}

#[cfg(test)]
mod test {

    use super::*;
    use chrono::Utc;
    use drogue_ttn as ttn;
    use drogue_ttn::v2::Metadata;
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

        let model_id = eval_data_schema(None, &device, &None, &uplink.port.to_string());

        assert_eq!(model_id, Some(String::from("mod5")));
    }

    #[test]
    fn test_model_no_mapping_1() {
        let device = device(None);
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &None, &uplink.port.to_string());

        assert_eq!(model_id, None);
    }

    #[test]
    fn test_model_no_mapping_2() {
        let device = device(Some(json!({
            "ports": { "1": {"data_schema": "mod1"}}
        })));
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &None, &uplink.port.to_string());

        assert_eq!(model_id, None);
    }

    #[test]
    fn test_model_no_mapping_3() {
        let device = device(Some(json!({
            "ports": { "1": {"no_data_schema": "mod1"}}
        })));
        let uplink = default_uplink(5);

        let model_id = eval_data_schema(None, &device, &None, &uplink.port.to_string());

        assert_eq!(model_id, None);
    }

    fn device(lorawan_spec: Option<Value>) -> registry::v1::Device {
        let mut spec = Map::new();
        if let Some(lorawan_spec) = lorawan_spec {
            spec.insert("lorawan".into(), lorawan_spec);
        }
        registry::v1::Device {
            metadata: Default::default(),
            spec,
            status: Default::default(),
        }
    }

    fn default_uplink(port: u16) -> ttn::v2::Uplink {
        ttn::v2::Uplink {
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
