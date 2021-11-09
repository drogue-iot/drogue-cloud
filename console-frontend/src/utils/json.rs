use http::Response;
use std::ops::{Deref, DerefMut};
use yew::format::Text;

pub struct Json<T>(pub T);

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Succeeded<D, T> {
    pub data: D,
    pub value: T,
}

impl<D, T> Succeeded<D, T> {
    pub fn new(data: D, value: T) -> Self {
        Self { data, value }
    }
}

impl<D, T> Deref for Succeeded<D, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<D, T> DerefMut for Succeeded<D, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

pub struct Failed<D, E> {
    pub data: D,
    pub error: E,
}

impl<D, E> Failed<D, E> {
    pub fn new(data: D, error: E) -> Self {
        Self { data, error }
    }
}

pub type ResponseResult<D, T> = std::result::Result<Succeeded<D, T>, Failed<D, anyhow::Error>>;
pub type JsonResponse<T> = Response<Json<ResponseResult<Text, T>>>;

impl<'a, T> From<Json<&'a T>> for yew::format::Text
where
    T: serde::Serialize,
{
    fn from(json: Json<&'a T>) -> Self {
        serde_json::to_string(json.0).map_err(anyhow::Error::from)
    }
}

impl<T> From<yew::format::Text> for Json<ResponseResult<yew::format::Text, T>>
where
    T: for<'de> serde::Deserialize<'de>,
{
    fn from(text: yew::format::Text) -> Self {
        match text {
            // we have text, and need to parse it
            Ok(ref s) => Json(match serde_json::from_str::<T>(s) {
                Ok(value) => Ok(Succeeded::new(text, value)),
                Err(reason) => Err(Failed::new(text, reason.into())),
            }),
            // we don't even have text
            Err(reason) => Json(Err(Failed::new(Ok(String::new()), reason))),
        }
    }
}

impl<'a, T> From<Json<&'a T>> for yew::format::Binary
where
    T: serde::Serialize,
{
    fn from(json: Json<&'a T>) -> Self {
        serde_json::to_vec(json.0).map_err(anyhow::Error::from)
    }
}
