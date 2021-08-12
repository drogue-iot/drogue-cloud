mod config;

pub use self::config::*;

use drogue_client::{registry, Translator};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Clone, Debug)]
pub enum ResourceType {
    Events(String),
    Commands(String),
    Users(String),
    Passwords(String),
}

impl ResourceType {
    pub fn app_name(&self) -> &str {
        match self {
            Self::Commands(app) => app.as_str(),
            Self::Events(app) => app.as_str(),
            Self::Users(app) => app.as_str(),
            Self::Passwords(app) => app.as_str(),
        }
    }
}

#[derive(Clone, Debug, Copy)]
pub enum KafkaEventType {
    Commands,
    Events,
}

impl KafkaEventType {
    fn make_topic(&self, name: String) -> String {
        make_kafka_resource_name(match self {
            Self::Commands => ResourceType::Commands(name),
            Self::Events => ResourceType::Events(name),
        })
    }
}

#[derive(Clone, Debug)]
pub enum KafkaTarget {
    Internal { topic: String },
    External { config: KafkaConfig },
}

pub trait KafkaConfigExt {
    type Error;

    /// Get a Kafka target, this can be either internal or external.
    ///
    /// This method must only return an Internal topic from a trusted source. Otherwise the user
    /// could internally redirect traffic.
    fn kafka_target(&self, event_type: KafkaEventType) -> Result<KafkaTarget, Self::Error>;

    /// Get a Kafka config, this can be either internal or external.
    ///
    /// This method must only return an Internal topic from a trusted source. Otherwise the user
    /// could internally redirect traffic.
    fn kafka_config(
        &self,
        event_type: KafkaEventType,
        default_kafka: &KafkaClientConfig,
    ) -> Result<KafkaConfig, Self::Error> {
        match self.kafka_target(event_type)? {
            KafkaTarget::External { config } => Ok(config),
            KafkaTarget::Internal { topic } => Ok(KafkaConfig {
                client: default_kafka.clone(),
                topic,
            }),
        }
    }
}

impl KafkaConfigExt for registry::v1::Application {
    type Error = serde_json::Error;

    fn kafka_target(&self, event_type: KafkaEventType) -> Result<KafkaTarget, Self::Error> {
        let status = self.section::<registry::v1::KafkaAppStatus>().transpose()?;

        Ok(match status.and_then(|s| s.downstream) {
            Some(status) => KafkaTarget::External {
                config: KafkaConfig {
                    client: KafkaClientConfig {
                        bootstrap_servers: status.bootstrap_servers,
                        properties: status.properties,
                    },
                    topic: status.topic,
                },
            },
            None => KafkaTarget::Internal {
                topic: event_type.make_topic(self.metadata.name.clone()),
            },
        })
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
    if name.len() < MAX_NAME_LEN && NAME_PATTERN.is_match(&resource) {
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
    let name = match &target {
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
            assert_eq!(
                i.1,
                make_kafka_resource_name(ResourceType::Events(i.0.to_string()))
            )
        }
    }
}
