use super::*;
use std::borrow::Cow;

/// Azure IoT dialect.
pub struct Azure;

impl ConnectValidator for Azure {
    fn validate_connect(&self, _connect: &Connect) -> Result<(), ServerError> {
        // we accept everything
        Ok(())
    }
}

impl PublishTopicParser for Azure {
    fn parse_publish<'a>(&self, path: &'a str) -> Result<ParsedPublishTopic<'a>, ParseError> {
        let (channel, properties) = split_topic(path);

        if channel.is_empty() {
            return Err(ParseError::Empty);
        }

        log::debug!("Azure: {channel} - properties: {properties:?}");

        Ok(ParsedPublishTopic {
            channel,
            device: None,
            properties,
        })
    }
}

impl SubscribeTopicParser for Azure {
    fn parse_subscribe<'a>(&self, path: &'a str) -> Result<ParsedSubscribeTopic<'a>, ParseError> {
        log::debug!("Azure: {path}");
        Ok(ParsedSubscribeTopic {
            filter: SubscribeFilter {
                device: DeviceFilter::Device,
                command: Some(path),
            },
            encoder: SubscriptionTopicEncoder::new(PlainTopicEncoder),
        })
    }
}

/// Split an Azure topic, which might carry a "bag of properties" as the last topic segment
pub fn split_topic(path: &str) -> (&str, Vec<(Cow<str>, Cow<str>)>) {
    if let Some((topic, last)) = path.rsplit_once('/') {
        // at least two segments
        if last.starts_with("?") {
            // last one is a bag of properties
            let query = url::form_urlencoded::parse(&last.as_bytes()[1..]);
            (topic, query.collect())
        } else {
            // last one is a regular one
            (path.trim_end_matches('/'), vec![])
        }
    } else {
        // single topic segment
        (path, vec![])
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_plain() {
        assert_eq!(split_topic("foo/bar"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_plain_slash() {
        assert_eq!(split_topic("foo/bar/"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_plain_slash_q() {
        assert_eq!(split_topic("foo/bar/?"), ("foo/bar", vec![]));
    }

    #[test]
    fn test_properties() {
        assert_eq!(
            split_topic("foo/bar/?baz=123"),
            ("foo/bar", vec![("baz".into(), "123".into())])
        );
    }
}
