mod wot;

pub use wot::*;

use drogue_client::registry::v1::MqttDialect;
use drogue_cloud_endpoint_common::command::{Command, CommandFilter};
use drogue_cloud_service_common::Id;
use std::fmt::Debug;
use std::ops::Deref;
use thiserror::Error;

/// A topic parser for the default session.
pub trait DefaultTopicParser {
    /// Parse a topic from a PUB request
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError>;

    /// Parse a topic from a SUB request
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError>;
}

#[derive(Debug)]
pub struct SubscriptionTopicEncoder(Box<dyn TopicEncoder>);

impl Deref for SubscriptionTopicEncoder {
    type Target = dyn TopicEncoder;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl SubscriptionTopicEncoder {
    pub fn new<T>(encoder: T) -> Self
    where
        T: TopicEncoder + 'static,
    {
        Self(Box::new(encoder))
    }
}

pub trait TopicEncoder: Debug {
    /// Encode a topic from a command, requested originally by a SUB request
    fn encode_command_topic(&self, command: &Command) -> String;
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

#[derive(Debug)]
pub struct DefaultCommandTopicEncoder(bool);

impl DefaultTopicParser for MqttDialect {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        match self {
            Self::DrogueV1 => {
                // This should mimic the behavior of the current parser
                let topic = path.split('/').collect::<Vec<_>>();
                log::debug!("Topic: {:?}", topic);

                match topic.as_slice() {
                    [""] => Err(ParseError::Empty),
                    [channel] => Ok(ParsedPublishTopic {
                        channel,
                        device: None,
                    }),
                    [channel, as_device] => Ok(ParsedPublishTopic {
                        channel,
                        device: Some(as_device),
                    }),
                    _ => Err(ParseError::Syntax),
                }
            }
            Self::PlainTopic {
                device_prefix: false,
            } => {
                // Plain topic, just take the complete path
                match path {
                    "" => Err(ParseError::Empty),
                    path => Ok(ParsedPublishTopic {
                        channel: path,
                        device: None,
                    }),
                }
            }
            Self::PlainTopic {
                device_prefix: true,
            } => {
                // Plain topic (with device prefix). Strip the device, and then just take the complete path

                let topic = path.split_once('/');
                log::debug!("Topic: {:?}", topic);

                match topic {
                    // No topic at all
                    None if path.is_empty() => Err(ParseError::Empty),
                    None => Err(ParseError::Syntax),
                    Some(("", path)) => Ok(ParsedPublishTopic {
                        channel: path,
                        device: None,
                    }),
                    Some((device, path)) => Ok(ParsedPublishTopic {
                        channel: path,
                        device: Some(device),
                    }),
                }
            }
            Self::WebOfThings { .. } => {
                let topic = path.split_once('/');
                log::debug!("Topic: {:?}", topic);

                match topic {
                    // No topic at all
                    None if path.is_empty() => Err(ParseError::Empty),
                    None => Ok(ParsedPublishTopic {
                        channel: "",
                        device: Some(path),
                    }),
                    Some(("", _)) => Err(ParseError::Syntax),
                    Some((device, path)) => Ok(ParsedPublishTopic {
                        channel: path,
                        device: Some(device),
                    }),
                }
            }
            Self::Cumulocity => {
                log::debug!("c8y: {path}");
                Err(ParseError::Syntax)
            }
        }
    }

    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        // TODO: replace .collect() with .as_slice() after "split_as_slice" #96137
        match self {
            Self::DrogueV1 | Self::PlainTopic { .. } => {
                match path.split('/').collect::<Vec<_>>().as_slice() {
                    // subscribe to commands for all proxied devices and ourself
                    ["command", "inbox", "#"] | ["command", "inbox", "+", "#"] => {
                        Ok(ParsedSubscribeTopic {
                            filter: SubscribeFilter {
                                device: DeviceFilter::Wildcard,
                                command: None,
                            },
                            encoder: SubscriptionTopicEncoder::new(DefaultCommandTopicEncoder(
                                false,
                            )),
                        })
                    }
                    // subscribe to commands directly for us
                    ["command", "inbox", "", "#"] => Ok(ParsedSubscribeTopic {
                        filter: SubscribeFilter {
                            device: DeviceFilter::Device,
                            command: None,
                        },
                        encoder: SubscriptionTopicEncoder::new(DefaultCommandTopicEncoder(false)),
                    }),
                    // subscribe to commands for a specific device
                    ["command", "inbox", device, "#"] => Ok(ParsedSubscribeTopic {
                        filter: SubscribeFilter {
                            device: DeviceFilter::ProxiedDevice(device),
                            command: None,
                        },
                        encoder: SubscriptionTopicEncoder::new(DefaultCommandTopicEncoder(true)),
                    }),
                    _ => Err(ParseError::Syntax),
                }
            }
            Self::WebOfThings { node_wot_bug } => match path.split_once('/') {
                Some((device, filter)) => Ok(ParsedSubscribeTopic {
                    filter: SubscribeFilter {
                        device: DeviceFilter::ProxiedDevice(device),
                        command: Some(filter),
                    },
                    encoder: SubscriptionTopicEncoder::new(WoTCommandTopicEncoder {
                        node_wot_bug: *node_wot_bug,
                    }),
                }),
                _ => Err(ParseError::Syntax),
            },
            Self::Cumulocity => {
                log::debug!("c8y: {path}");
                Err(ParseError::Syntax)
            }
        }
    }
}

impl TopicEncoder for DefaultCommandTopicEncoder {
    fn encode_command_topic(&self, command: &Command) -> String {
        // if we are forced to report the device part, or the device id is not equal to the
        // connected device, then we need to add it.
        if self.0 || command.address.gateway_id != command.address.device_id {
            format!(
                "command/inbox/{}/{}",
                command.address.device_id, command.command
            )
        } else {
            format!("command/inbox//{}", command.command)
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
            }),
        );
        // channel for another device
        assert_parse(
            &spec,
            "foo/device",
            Ok(ParsedPublishTopic {
                channel: "foo",
                device: Some("device"),
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
            }),
        );
        assert_parse(
            &spec,
            "foo/bar",
            Ok(ParsedPublishTopic {
                channel: "foo/bar",
                device: None,
            }),
        );
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedPublishTopic {
                channel: "/bar",
                device: None,
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
            }),
        );
        // device may be empty though
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedPublishTopic {
                channel: "bar",
                device: None,
            }),
        );
        // longer topic
        assert_parse(
            &spec,
            "foo/bar/baz//bam/bum",
            Ok(ParsedPublishTopic {
                channel: "bar/baz//bam/bum",
                device: Some("foo"),
            }),
        );
    }

    fn assert_parse(spec: &MqttSpec, path: &str, expected: Result<ParsedPublishTopic, ParseError>) {
        assert_eq!(spec.dialect.parse_publish(path), expected);
    }
}
