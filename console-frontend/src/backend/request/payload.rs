use super::*;

pub trait RequestPayload {
    type Error: std::error::Error;
    fn into_js(self) -> Result<Option<JsValue>, Self::Error>;
    fn content_type(&self) -> Option<String>;
}

pub trait ResponsePayload {
    type Target;
    type Error: std::error::Error;
    fn convert_target(data: &[u8]) -> Result<Self::Target, Self::Error>;
}

pub struct Nothing;

impl RequestPayload for Nothing {
    type Error = Infallible;

    fn into_js(self) -> Result<Option<JsValue>, Self::Error> {
        Ok(None)
    }

    fn content_type(&self) -> Option<String> {
        None
    }
}

impl ResponsePayload for () {
    type Target = ();
    type Error = Infallible;

    fn convert_target(_: &[u8]) -> Result<Self::Target, Self::Error> {
        Ok(())
    }
}

pub struct Json<T>(pub T);

impl<T> ResponsePayload for Json<T>
where
    T: for<'de> Deserialize<'de>,
{
    type Target = T;
    type Error = serde_json::Error;

    fn convert_target(data: &[u8]) -> Result<Self::Target, Self::Error> {
        serde_json::from_slice(data)
    }
}

impl<T> ResponsePayload for Option<Json<T>>
where
    T: for<'de> Deserialize<'de>,
{
    type Target = Option<T>;
    type Error = serde_json::Error;

    fn convert_target(data: &[u8]) -> Result<Self::Target, Self::Error> {
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_slice(data)?))
        }
    }
}

impl<T> RequestPayload for Json<T>
where
    T: Serialize,
{
    type Error = serde_json::Error;

    fn into_js(self) -> Result<Option<JsValue>, Self::Error> {
        let data = serde_json::to_vec(&self.0)?;

        Ok(Some(Uint8Array::from(data.as_slice()).into()))
    }

    fn content_type(&self) -> Option<String> {
        Some("application/json".to_string())
    }
}
