use monaco::api::TextModel;
use wasm_bindgen::JsValue;

/// Convert content to YAML
pub fn to_yaml_model<T>(content: &T) -> Result<TextModel, JsValue>
where
    T: serde::Serialize,
{
    let yaml = serde_yaml::to_string(content).unwrap_or_default();
    let p: &[_] = &['-', '\n', '\r'];
    let yaml = yaml.trim_start_matches(p);
    TextModel::create(yaml, Some("yaml"), None)
}
