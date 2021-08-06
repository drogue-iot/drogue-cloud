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
    to_model(Some("yaml"), yaml)
}

/// Convert content to TextModel
pub fn to_model<S>(language: Option<&str>, text: S) -> Result<TextModel, JsValue>
where
    S: AsRef<str>,
{
    TextModel::create(text.as_ref(), language, None)
}
