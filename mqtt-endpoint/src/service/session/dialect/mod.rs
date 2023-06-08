mod az;
mod c8y;
mod drogue;
mod encoder;
mod plain;
mod wot;

pub use az::*;
pub use c8y::*;
pub use drogue::*;
pub use encoder::*;
pub use plain::*;
pub use wot::*;

use drogue_client::registry::v1::MqttDialect;
use drogue_cloud_endpoint_common::command::CommandFilter;
use drogue_cloud_mqtt_common::error::ServerError;
use drogue_cloud_mqtt_common::mqtt::Connect;
use drogue_cloud_service_common::Id;
use std::{borrow::Cow, fmt::Debug, sync::Arc};
use thiserror::Error;

pub trait ConnectValidator {
    fn validate_connect(&self, connect: &Connect) -> Result<(), ServerError>;
}

/// Reject cleanSession=false
pub struct RejectResumeSession;

impl ConnectValidator for RejectResumeSession {
    fn validate_connect(&self, connect: &Connect) -> Result<(), ServerError> {
        match connect.clean_session() {
            true => Ok(()),
            false => Err(ServerError::UnsupportedOperation),
        }
    }
}

pub trait PublishTopicParser {
    /// Parse a topic from a PUB request
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError>;
}

pub trait SubscribeTopicParser {
    /// Parse a topic from a SUB request
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("Topic syntax error")]
    Syntax,
    #[error("Empty topic error")]
    Empty,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedPublishTopic<'a> {
    pub channel: &'a str,
    pub device: Option<&'a str>,
    pub properties: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

#[derive(Debug)]
pub struct ParsedSubscribeTopic<'a> {
    pub filter: SubscribeFilter<'a>,
    pub encoder: SubscriptionTopicEncoder,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SubscribeFilter<'a> {
    pub device: DeviceFilter<'a>,
    pub command: Option<&'a str>,
}

impl SubscribeFilter<'_> {
    pub fn into_command_filter(self, id: &Id) -> CommandFilter {
        match self.device {
            DeviceFilter::Wildcard => CommandFilter::wildcard(&id.app_id, &id.device_id),
            DeviceFilter::Device => CommandFilter::device(&id.app_id, &id.device_id),
            DeviceFilter::ProxiedDevice(device) => {
                if device == id.device_id {
                    CommandFilter::device(&id.app_id, &id.device_id)
                } else {
                    CommandFilter::proxied_device(&id.app_id, &id.device_id, device)
                }
            }
        }
        .with_filter(self.command.map(|s| s.to_string()))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeviceFilter<'a> {
    /// All commands for the device and proxied devices
    Wildcard,
    /// All commands for the actual device
    Device,
    /// All commands for the specific device
    ProxiedDevice(&'a str),
}

pub trait DialectBuilder {
    fn create(&self) -> Dialect;
}

#[derive(Clone)]
pub struct Dialect {
    connect: Arc<dyn ConnectValidator>,
    publish: Arc<dyn PublishTopicParser>,
    subscribe: Arc<dyn SubscribeTopicParser>,
}

impl Dialect {
    pub fn new<C, P, S>(connect: C, publish: P, subscribe: S) -> Self
    where
        C: ConnectValidator + 'static,
        P: PublishTopicParser + 'static,
        S: SubscribeTopicParser + 'static,
    {
        Self {
            connect: Arc::new(connect),
            publish: Arc::new(publish),
            subscribe: Arc::new(subscribe),
        }
    }

    /// Parse a topic from a PUB request
    pub fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        self.publish.parse_publish(path)
    }

    /// Parse a topic from a SUB request
    pub fn parse_subscribe<'a>(
        &self,
        path: &'a str,
    ) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        self.subscribe.parse_subscribe(path)
    }

    pub fn validate_connect(&self, connect: &Connect) -> Result<(), ServerError> {
        self.connect.validate_connect(connect)
    }
}

impl<CSP> From<CSP> for Dialect
where
    CSP: ConnectValidator + PublishTopicParser + SubscribeTopicParser + 'static,
{
    fn from(composite: CSP) -> Self {
        let subscribe = Arc::new(composite);
        Self {
            connect: subscribe.clone(),
            publish: subscribe.clone(),
            subscribe,
        }
    }
}

impl<C, SP> From<(C, SP)> for Dialect
where
    C: ConnectValidator + 'static,
    SP: PublishTopicParser + SubscribeTopicParser + 'static,
{
    fn from((connect, composite): (C, SP)) -> Self {
        let subscribe = Arc::new(composite);
        Self {
            connect: Arc::new(connect),
            publish: subscribe.clone(),
            subscribe,
        }
    }
}

impl DialectBuilder for MqttDialect {
    fn create(&self) -> Dialect {
        match self {
            Self::DrogueV1 => (RejectResumeSession, DrogueV1).into(),
            Self::PlainTopic { device_prefix } => Dialect::new(
                RejectResumeSession,
                PlainTopic {
                    device_prefix: *device_prefix,
                },
                DrogueV1,
            ),
            Self::WebOfThings { node_wot_bug } => (
                RejectResumeSession,
                WebOfThings {
                    node_wot_bug: *node_wot_bug,
                },
            )
                .into(),
            Self::Cumulocity => (RejectResumeSession, Cumulocity).into(),
            Self::Azure => Azure.into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_client::registry::v1::MqttSpec;
    use serde_json::json;

    #[test]
    fn test_v1() {
        let spec: MqttSpec = serde_json::from_value(json!({"dialect":{
            "type": "drogue/v1"
        }}))
        .unwrap();

        assert_parse(&spec, "", Err(ParseError::Empty));
        // channel for self device
        assert_parse(
            &spec,
            "foo",
            Ok(ParsedPublishTopic {
                channel: "foo",
                device: None,
                properties: vec![],
            }),
        );
        // channel for another device
        assert_parse(
            &spec,
            "foo/device",
            Ok(ParsedPublishTopic {
                channel: "foo",
                device: Some("device"),
                properties: vec![],
            }),
        );
    }

    #[test]
    fn test_plain() {
        let spec: MqttSpec = serde_json::from_value(json!({"dialect":{
            "type": "plainTopic"
        }}))
        .unwrap();

        assert_parse(&spec, "", Err(ParseError::Empty));

        // just take the topic, the device is always `None`

        assert_parse(
            &spec,
            "foo",
            Ok(ParsedPublishTopic {
                channel: "foo",
                device: None,
                properties: vec![],
            }),
        );
        assert_parse(
            &spec,
            "foo/bar",
            Ok(ParsedPublishTopic {
                channel: "foo/bar",
                device: None,
                properties: vec![],
            }),
        );
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedPublishTopic {
                channel: "/bar",
                device: None,
                properties: vec![],
            }),
        );
    }

    #[test]
    fn test_plain_prefix() {
        let spec: MqttSpec = serde_json::from_value(json!({"dialect":{
            "type": "plainTopic",
            "devicePrefix": true,
        }}))
        .unwrap();

        assert_parse(&spec, "", Err(ParseError::Empty));
        // we need at least two segments
        assert_parse(&spec, "foo", Err(ParseError::Syntax));
        // check that device comes first
        assert_parse(
            &spec,
            "foo/bar",
            Ok(ParsedPublishTopic {
                channel: "bar",
                device: Some("foo"),
                properties: vec![],
            }),
        );
        // device may be empty though
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedPublishTopic {
                channel: "bar",
                device: None,
                properties: vec![],
            }),
        );
        // longer topic
        assert_parse(
            &spec,
            "foo/bar/baz//bam/bum",
            Ok(ParsedPublishTopic {
                channel: "bar/baz//bam/bum",
                device: Some("foo"),
                properties: vec![],
            }),
        );
    }

    #[test]
    fn test_azure_prefix() {
        let spec: MqttSpec = serde_json::from_value(json!({"dialect":{
            "type": "azure",
        }}))
        .unwrap();

        assert_parse(&spec, "", Err(ParseError::Empty));
        // simple
        assert_parse(
            &spec,
            "foo/bar/baz",
            Ok(ParsedPublishTopic {
                channel: "foo/bar/baz",
                device: None,
                properties: vec![],
            }),
        );
        // properties
        assert_parse(
            &spec,
            "foo/bar/baz/?foo=bar&bar=baz",
            Ok(ParsedPublishTopic {
                channel: "foo/bar/baz",
                device: None,
                properties: vec![("foo".into(), "bar".into()), ("bar".into(), "baz".into())],
            }),
        );
    }

    fn assert_parse(spec: &MqttSpec, path: &str, expected: Result<ParsedPublishTopic, ParseError>) {
        assert_eq!(spec.dialect.create().parse_publish(path), expected);
    }
}
