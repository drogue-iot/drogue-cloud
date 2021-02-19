use bytes::{Bytes, BytesMut};
use cloudevents::event::SpecVersion;
use cloudevents::message::{
    BinaryDeserializer, BinarySerializer, Encoding, MessageAttributeValue, MessageDeserializer,
    Result, StructuredDeserializer, StructuredSerializer,
};
use cloudevents::{message, Event};
use futures::StreamExt;
use http::{header, header::HeaderName, HeaderValue};
use lazy_static::lazy_static;
use ntex::{http::HttpMessage, web, web::HttpRequest};
use std::convert::TryFrom;

use std::collections::HashMap;
use std::str::FromStr;

macro_rules! unwrap_optional_header {
    ($headers:expr, $name:expr) => {
        $headers
            .get::<&'static HeaderName>(&$name)
            .map(|a| header_value_to_str!(a))
    };
}

macro_rules! header_value_to_str {
    ($header_value:expr) => {
        $header_value
            .to_str()
            .map_err(|e| cloudevents::message::Error::Other {
                source: Box::new(e),
            })
    };
}

macro_rules! str_name_to_header {
    ($attribute:expr) => {
        HeaderName::from_str($attribute).map_err(|e| cloudevents::message::Error::Other {
            source: Box::new(e),
        })
    };
}

macro_rules! attribute_name_to_header {
    ($attribute:expr) => {
        str_name_to_header!(&["ce-", $attribute].concat())
    };
}

fn attributes_to_headers(
    it: impl Iterator<Item = &'static str>,
) -> HashMap<&'static str, HeaderName> {
    it.map(|s| {
        if s == "datacontenttype" {
            (s, header::CONTENT_TYPE)
        } else {
            (s, attribute_name_to_header!(s).unwrap())
        }
    })
    .collect()
}

lazy_static! {
    pub(crate) static ref ATTRIBUTES_TO_HEADERS: HashMap<&'static str, HeaderName> =
        attributes_to_headers(SpecVersion::all_attribute_names());
    pub(crate) static ref SPEC_VERSION_HEADER: HeaderName =
        HeaderName::from_static("ce-specversion");
    pub(crate) static ref CLOUDEVENTS_JSON_HEADER: HeaderValue =
        HeaderValue::from_static("application/cloudevents+json");
}

/// Wrapper for [`HttpRequest`] that implements [`MessageDeserializer`] trait.
pub struct HttpRequestDeserializer<'a> {
    req: &'a HttpRequest,
    body: Bytes,
}

impl HttpRequestDeserializer<'_> {
    pub fn new(req: &HttpRequest, body: Bytes) -> HttpRequestDeserializer {
        HttpRequestDeserializer { req, body }
    }
}

impl<'a> BinaryDeserializer for HttpRequestDeserializer<'a> {
    fn deserialize_binary<R: Sized, V: BinarySerializer<R>>(self, mut visitor: V) -> Result<R> {
        if self.encoding() != Encoding::BINARY {
            return Err(message::Error::WrongEncoding {});
        }

        let spec_version = SpecVersion::try_from(
            unwrap_optional_header!(self.req.headers(), SPEC_VERSION_HEADER).unwrap()?,
        )?;

        visitor = visitor.set_spec_version(spec_version.clone())?;

        let attributes = spec_version.attribute_names();

        for (hn, hv) in self
            .req
            .headers()
            .iter()
            .filter(|(hn, _)| SPEC_VERSION_HEADER.ne(hn) && hn.as_str().starts_with("ce-"))
        {
            let name = &hn.as_str()["ce-".len()..];

            if attributes.contains(&name) {
                visitor = visitor.set_attribute(
                    name,
                    MessageAttributeValue::String(String::from(header_value_to_str!(hv)?)),
                )?
            } else {
                visitor = visitor.set_extension(
                    name,
                    MessageAttributeValue::String(String::from(header_value_to_str!(hv)?)),
                )?
            }
        }

        if let Some(hv) = self.req.headers().get("content-type") {
            visitor = visitor.set_attribute(
                "datacontenttype",
                MessageAttributeValue::String(String::from(header_value_to_str!(hv)?)),
            )?
        }

        if !self.body.is_empty() {
            visitor.end_with_data(self.body.to_vec())
        } else {
            visitor.end()
        }
    }
}

impl<'a> StructuredDeserializer for HttpRequestDeserializer<'a> {
    fn deserialize_structured<R: Sized, V: StructuredSerializer<R>>(self, visitor: V) -> Result<R> {
        if self.encoding() != Encoding::STRUCTURED {
            return Err(message::Error::WrongEncoding {});
        }
        visitor.set_structured_event(self.body.to_vec())
    }
}

impl<'a> MessageDeserializer for HttpRequestDeserializer<'a> {
    fn encoding(&self) -> Encoding {
        if self.req.content_type() == "application/cloudevents+json" {
            Encoding::STRUCTURED
        } else if self
            .req
            .headers()
            .get::<&'static HeaderName>(&SPEC_VERSION_HEADER)
            .is_some()
        {
            Encoding::BINARY
        } else {
            Encoding::UNKNOWN
        }
    }
}

/// Method to transform an incoming [`HttpRequest`] to [`Event`].
pub async fn request_to_event(
    req: &HttpRequest,
    mut payload: web::types::Payload,
) -> std::result::Result<Event, ntex::web::error::PayloadError> {
    let mut bytes = BytesMut::new();
    while let Some(item) = payload.next().await {
        bytes.extend_from_slice(&item?);
    }
    MessageDeserializer::into_event(HttpRequestDeserializer::new(req, bytes.freeze()))
        .map_err(|_| web::error::PayloadError::Decoding)
}
