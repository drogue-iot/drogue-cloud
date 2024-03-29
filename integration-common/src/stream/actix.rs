use crate::stream::EventStream;
use actix_web::{
    http::StatusCode,
    web::{Bytes, BytesMut},
};
use drogue_cloud_event_common::stream;
use futures::{future, Stream, TryStreamExt};
use std::pin::Pin;

/// create an SSE frame from an even already in string format
fn make_frame(event: String) -> Bytes {
    let mut r = BytesMut::new();

    r.extend(b"data: ");
    r.extend(event.as_bytes());
    r.extend(b"\n\n");

    r.freeze()
}

pub fn map_to_sse<'s>(
    event_stream: EventStream<'s>,
) -> impl Stream<Item = Result<Bytes, actix_web::Error>> + 's {
    let event_stream: stream::EventStream<'s> = event_stream.into();
    event_stream
        .map_err(|err| {
            log::debug!("Failed to process event: {}", err);
            actix_web::error::InternalError::new(err, StatusCode::INTERNAL_SERVER_ERROR).into()
        })
        .and_then(|event| {
            let r = serde_json::to_string(&event)
                .map(make_frame)
                .map_err(|err| {
                    actix_web::error::InternalError::new(err, StatusCode::INTERNAL_SERVER_ERROR)
                        .into()
                });
            future::ready(r)
        })
}

pub trait IntoSseStream<'s> {
    fn into_sse_stream(self) -> Pin<Box<dyn Stream<Item = Result<Bytes, actix_web::Error>> + 's>>;
}

impl<'s> IntoSseStream<'s> for EventStream<'s> {
    fn into_sse_stream(self) -> Pin<Box<dyn Stream<Item = Result<Bytes, actix_web::Error>> + 's>> {
        Box::pin(map_to_sse(self))
    }
}
