use super::*;

/// Plain topic dialect.
pub struct PlainTopic {
    pub device_prefix: bool,
}

impl PublishTopicParser for PlainTopic {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        match self.device_prefix {
            true => {
                // Plain topic (with device prefix). Strip the device, and then just take the complete path

                let topic = path.split_once('/');
                log::debug!("Topic: {topic:?}",);

                match topic {
                    // No topic at all
                    None if path.is_empty() => Err(ParseError::Empty),
                    None => Err(ParseError::Syntax),
                    Some(("", path)) => Ok(ParsedPublishTopic {
                        channel: path,
                        device: None,
                        properties: vec![],
                    }),
                    Some((device, path)) => Ok(ParsedPublishTopic {
                        channel: path,
                        device: Some(device),
                        properties: vec![],
                    }),
                }
            }
            false => {
                // Plain topic, just take the complete path
                match path {
                    "" => Err(ParseError::Empty),
                    path => Ok(ParsedPublishTopic {
                        channel: path,
                        device: None,
                        properties: vec![],
                    }),
                }
            }
        }
    }
}
