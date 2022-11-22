use super::Command;
use mqtt::{TopicFilter, TopicNameRef};

use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct CommandTarget {
    pub tx: Sender<Command>,
    /// an additional command filter, in the form of an MQTT topic filter
    pub filter: CommandNameFilter,
}

#[derive(Clone, Debug)]
pub enum CommandNameFilter {
    /// always pass
    Always,
    /// never pass
    Never,
    /// MQTT topic filter
    Filter(TopicFilter),
}

impl CommandNameFilter {
    pub fn from(filter: &Option<String>) -> Self {
        match filter {
            Some(filter) => match TopicFilter::new(filter.to_string()) {
                Ok(filter) => Self::Filter(filter),
                Err(_) => Self::Never,
            },
            None => Self::Always,
        }
    }

    /// Match the command filter against the provided command name.
    ///
    /// If no filter is present, the command will always match.
    pub fn matches(&self, command: &str) -> bool {
        match &self {
            Self::Filter(filter) => {
                if let Ok(name) = TopicNameRef::new(&command) {
                    filter.get_matcher().is_match(name)
                } else {
                    false
                }
            }
            Self::Never => false,
            Self::Always => true,
        }
    }
}
