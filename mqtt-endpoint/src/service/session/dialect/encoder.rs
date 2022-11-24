use drogue_cloud_endpoint_common::command::Command;
use std::fmt::Debug;
use std::ops::Deref;

/// A structure boxing a dynamic [`TopicEncoder`] instance.
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

/// An encoder, from commands to topic names.
///
/// This carries over the information from the subscription request, to the encoding of topic names
/// for received commands.
pub trait TopicEncoder: Debug {
    /// Encode a topic from a command, requested originally by a SUB request
    fn encode_command_topic(&self, command: &Command) -> String;
}

/// An encoder which uses the plain command name as topic.
#[derive(Debug)]
pub struct PlainTopicEncoder;

impl TopicEncoder for PlainTopicEncoder {
    fn encode_command_topic(&self, command: &Command) -> String {
        command.command.clone()
    }
}
