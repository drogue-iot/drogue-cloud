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

/// An encoder which uses the plain command name as topic.
#[derive(Debug)]
pub struct PlainTopicEncoder;

impl TopicEncoder for PlainTopicEncoder {
    fn encode_command_topic(&self, command: &Command) -> String {
        command.command.clone()
    }
}
