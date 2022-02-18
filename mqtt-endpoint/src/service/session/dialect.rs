use drogue_client::registry::v1::MqttDialect;
use thiserror::Error;

/// A topic parser for the default session.
pub trait DefaultTopicParser {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedTopic<'a>, ParseError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("Topic syntax error")]
    Syntax,
    #[error("Empty topic error")]
    Empty,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedTopic<'a> {
    pub channel: &'a str,
    pub device: Option<&'a str>,
}

impl DefaultTopicParser for MqttDialect {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedTopic<'a>, ParseError> {
        match &self {
            Self::DrogueV1 => {
                // This should mimic the behavior of the current parser
                let topic = path.split('/').collect::<Vec<_>>();
                log::debug!("Topic: {:?}", topic);

                match topic.as_slice() {
                    [""] => Err(ParseError::Empty),
                    [channel] => Ok(ParsedTopic {
                        channel,
                        device: None,
                    }),
                    [channel, as_device] => Ok(ParsedTopic {
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
                    path => Ok(ParsedTopic {
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
                    None if path == "" => Err(ParseError::Empty),
                    None => Err(ParseError::Syntax),
                    Some(("", path)) => Ok(ParsedTopic {
                        channel: path,
                        device: None,
                    }),
                    Some((device, path)) => Ok(ParsedTopic {
                        channel: path,
                        device: Some(device),
                    }),
                }
            }
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
            Ok(ParsedTopic {
                channel: "foo",
                device: None,
            }),
        );
        // channel for another device
        assert_parse(
            &spec,
            "foo/device",
            Ok(ParsedTopic {
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
            Ok(ParsedTopic {
                channel: "foo",
                device: None,
            }),
        );
        assert_parse(
            &spec,
            "foo/bar",
            Ok(ParsedTopic {
                channel: "foo/bar",
                device: None,
            }),
        );
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedTopic {
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
            Ok(ParsedTopic {
                channel: "bar",
                device: Some("foo"),
            }),
        );
        // device may be empty though
        assert_parse(
            &spec,
            "/bar",
            Ok(ParsedTopic {
                channel: "bar",
                device: None,
            }),
        );
        // longer topic
        assert_parse(
            &spec,
            "foo/bar/baz//bam/bum",
            Ok(ParsedTopic {
                channel: "bar/baz//bam/bum",
                device: Some("foo"),
            }),
        );
    }

    fn assert_parse(spec: &MqttSpec, path: &str, expected: Result<ParsedTopic, ParseError>) {
        assert_eq!(spec.dialect.parse_publish(path), expected);
    }
}
