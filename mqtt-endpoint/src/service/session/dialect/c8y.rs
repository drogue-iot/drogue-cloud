use super::*;

/// Cumulocity dialect.
///
/// NOTE: This is experimental.
pub struct Cumulocity;

impl PublishTopicParser for Cumulocity {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        let topic = path.split('/').collect::<Vec<_>>();
        log::debug!("C8Y: {topic:?}",);

        match topic.as_slice() {
            [""] => Err(ParseError::Empty),
            ["s", "us"] => Ok(ParsedPublishTopic {
                channel: "c8y",
                device: None,
                properties: vec![],
            }),
            ["s", "us", as_device] => Ok(ParsedPublishTopic {
                channel: "c8y",
                device: Some(as_device),
                properties: vec![],
            }),
            _ => Err(ParseError::Syntax),
        }
    }
}

impl SubscribeTopicParser for Cumulocity {
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        log::debug!("c8y: {path}");
        match path.split('/').collect::<Vec<_>>().as_slice() {
            [] => Err(ParseError::Empty),
            ["s", "e"] => Ok(ParsedSubscribeTopic {
                filter: SubscribeFilter {
                    device: DeviceFilter::Device,
                    command: None,
                },
                encoder: SubscriptionTopicEncoder::new(DefaultCommandTopicEncoder(false)),
            }),
            _ => Err(ParseError::Syntax),
        }
    }
}
