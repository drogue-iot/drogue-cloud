use super::*;

use crate::service::session::dialect::TopicEncoder;
use drogue_cloud_endpoint_common::command::Command;

/// Web of Things dialect.
///
/// NOTE: This is experimental.
pub struct WebOfThings {
    pub node_wot_bug: bool,
}

impl PublishTopicParser for WebOfThings {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        let topic = path.split_once('/');
        log::debug!("Topic: {topic:?}",);

        match topic {
            // No topic at all
            None if path.is_empty() => Err(ParseError::Empty),
            None => Ok(ParsedPublishTopic {
                channel: "",
                device: Some(path),
                properties: vec![],
            }),
            Some(("", _)) => Err(ParseError::Syntax),
            Some((device, path)) => Ok(ParsedPublishTopic {
                channel: path,
                device: Some(device),
                properties: vec![],
            }),
        }
    }
}

impl SubscribeTopicParser for WebOfThings {
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        match path.split_once('/') {
            Some((device, filter)) => Ok(ParsedSubscribeTopic {
                filter: SubscribeFilter {
                    device: DeviceFilter::ProxiedDevice(device),
                    command: Some(filter),
                },
                encoder: SubscriptionTopicEncoder::new(WoTCommandTopicEncoder {
                    node_wot_bug: self.node_wot_bug,
                }),
            }),
            _ => Err(ParseError::Syntax),
        }
    }
}

#[derive(Debug)]
pub struct WoTCommandTopicEncoder {
    pub node_wot_bug: bool,
}

/// Encodes the topic simply as device + command name
impl TopicEncoder for WoTCommandTopicEncoder {
    fn encode_command_topic(&self, command: &Command) -> String {
        if self.node_wot_bug {
            // yes, this is weird
            format!("/{}/{}", command.address.device_id, command.command)
        } else {
            format!("{}/{}", command.address.device_id, command.command)
        }
    }
}
