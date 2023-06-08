use super::*;
use drogue_cloud_endpoint_common::command::Command;

/// Drogue IoT v1 dialect.
pub struct DrogueV1;

impl PublishTopicParser for DrogueV1 {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        // This should mimic the behavior of the current parser
        let topic = path.split('/').collect::<Vec<_>>();
        log::debug!("Topic: {topic:?}",);

        match topic.as_slice() {
            [""] => Err(ParseError::Empty),
            [channel] => Ok(ParsedPublishTopic {
                channel,
                device: None,
                properties: vec![],
            }),
            [channel, as_device] => Ok(ParsedPublishTopic {
                channel,
                device: Some(as_device),
                properties: vec![],
            }),
            _ => Err(ParseError::Syntax),
        }
    }
}

impl SubscribeTopicParser for DrogueV1 {
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        match path.split('/').collect::<Vec<_>>().as_slice() {
            // subscribe to commands for all proxied devices and ourself
            ["command", "inbox", "#"] | ["command", "inbox", "+", "#"] => {
                Ok(ParsedSubscribeTopic {
                    filter: SubscribeFilter {
                        device: DeviceFilter::Wildcard,
                        command: None,
                    },
                    encoder: SubscriptionTopicEncoder::new(DefaultCommandTopicEncoder(false)),
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
}

/// The default (Drogue V1) encoder, which expects the command inbox pattern.
#[derive(Debug)]
pub struct DefaultCommandTopicEncoder(pub bool);

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
