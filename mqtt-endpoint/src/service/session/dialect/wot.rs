use crate::service::session::dialect::TopicEncoder;
use drogue_cloud_endpoint_common::command::Command;

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
