use drogue_cloud_integration_common::{self, stream::EventStream};
use ntex::util::ByteString;
use std::num::NonZeroU32;

pub struct Stream<'s> {
    pub topic: ByteString,
    pub id: Option<NonZeroU32>,
    pub event_stream: EventStream<'s>,
    pub content_mode: ContentMode,
}

impl Drop for Stream<'_> {
    fn drop(&mut self) {
        log::info!("Dropped stream - topic: {}", self.topic);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ContentMode {
    Binary,
    Structured,
}
