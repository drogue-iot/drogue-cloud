mod config;

pub use self::config::*;
use std::convert::Infallible;

use drogue_client::registry;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Clone, Debug)]
pub enum ResourceType<'a> {
    Events(&'a str),
    Commands(&'a str),
    Users(&'a str),
    Passwords(&'a str),
}

impl<'a> ResourceType<'a> {
    pub fn app_name(&self) -> &str {
        match self {
            Self::Commands(app) => app,
            Self::Events(app) => app,
            Self::Users(app) => app,
            Self::Passwords(app) => app,
        }
    }
}

#[derive(Clone, Debug, Copy)]
pub enum KafkaEventType {
    Commands,
    Events,
}

impl KafkaEventType {
    fn make_topic(&self, name: &str) -> String {
        make_kafka_resource_name(match self {
            Self::Commands => ResourceType::Commands(name),
            Self::Events => ResourceType::Events(name),
        })
    }
}

#[derive(Clone, Debug)]
pub struct KafkaTarget<'a> {
    pub client: &'a KafkaClientConfig,
    pub topic: String,
}

impl<'a> From<KafkaTarget<'a>> for KafkaConfig {
    fn from(target: KafkaTarget<'a>) -> Self {
        KafkaConfig {
            client: target.client.clone(),
            topic: target.topic,
        }
    }
}

pub trait KafkaConfigExt {
    type Error;

    /// Get a Kafka topic..
    ///
    /// This method must only return an Internal topic from a trusted source. Otherwise the user
    /// could internally redirect traffic.
    fn kafka_topic(&self, event_type: KafkaEventType) -> Result<String, Self::Error>;

    /// Get a Kafka config, this can be either internal or external.
    ///
    /// This method must only return an Internal topic from a trusted source. Otherwise the user
    /// could internally redirect traffic.
    fn kafka_target<'a>(
        &self,
        event_type: KafkaEventType,
        default_kafka: &'a KafkaClientConfig,
    ) -> Result<KafkaTarget<'a>, Self::Error> {
        Ok(KafkaTarget {
            client: default_kafka,
            topic: self.kafka_topic(event_type)?,
        })
    }
}

impl KafkaConfigExt for registry::v1::Application {
    type Error = Infallible;

    fn kafka_topic(&self, event_type: KafkaEventType) -> Result<String, Self::Error> {
        Ok(event_type.make_topic(&self.metadata.name))
    }
}

const MAX_NAME_LEN: usize = 63;

const NAME_REGEXP: &str = r#"^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$"#;
lazy_static! {
    static ref NAME_PATTERN: Regex = Regex::new(NAME_REGEXP).expect("Regexp must compile");
}

fn resource_name(prefix: &str, hashed_prefix: &str, resource: &str) -> String {
    let name = format!("{}-{}", prefix, resource);
    // try the simple route, if that works ...
    if name.len() < MAX_NAME_LEN && NAME_PATTERN.is_match(resource) {
        // ... simply return
        name
    } else {
        // otherwise we need to clean up the name, and ensure we don't generate duplicates
        // use a different prefix to prevent clashes with the simple names
        let hash = md5::compute(resource);
        format!("{}-{:x}-{}", hashed_prefix, hash, resource)
    }
}

pub fn make_kafka_resource_name(target: ResourceType) -> String {
    let name = match target {
        ResourceType::Events(app) => resource_name("events", "evt", app),
        ResourceType::Users(app) => resource_name("user", "usr", app),
        ResourceType::Passwords(app) => resource_name("password", "pwd", app),
        ResourceType::Commands(_) => return "iot-commands".to_string(),
    };

    let name: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '-' | 'a'..='z' | '0'..='9' => c,
            _ => '-',
        })
        .take(MAX_NAME_LEN)
        .collect();

    name.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn topic_names() {
        for i in [
            ("foo", "events-foo"),
            ("00foo", "events-00foo"),
            (
                "0123456789012345678901234567890123456789012345678901234567890123456789",
                "evt-109eb12c10c45d94ddac8eca7b818bed-01234567890123456789012345",
            ),
            ("FOO", "evt-901890a8e9c8cf6d5a1a542b229febff-foo"),
            ("foo-", "evt-03f19ca8da08c40c2d036c8915d383e2-foo"),
        ] {
            assert_eq!(i.1, make_kafka_resource_name(ResourceType::Events(i.0)))
        }
    }
}
