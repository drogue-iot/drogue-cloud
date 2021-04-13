mod v2;

pub use v2::*;

use drogue_cloud_service_api::management::Device;
use drogue_ttn::http as ttn;
use serde_json::Value;

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
