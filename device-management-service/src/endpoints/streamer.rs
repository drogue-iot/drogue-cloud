use bytes::{BufMut, Bytes, BytesMut};
use futures::{
    task::{Context, Poll},
    {ready, Stream},
};
use pin_project::pin_project;
use serde::Serialize;
use std::{error::Error, pin::Pin};

/// The internal state of the stream
enum State {
    /// Before the first item
    Start,
    /// In the middle of processing
    Data,
    /// After the last item
    End,
}

#[pin_project]
pub struct ArrayStreamer<S, T, E>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Error + 'static,
{
    #[pin]
    stream: S,
    state: State,
}

impl<S, T, E> ArrayStreamer<S, T, E>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Error + 'static,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            state: State::Start,
        }
    }
}

impl<S, T, E> Stream for ArrayStreamer<S, T, E>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Error + 'static,
{
    type Item = Result<Bytes, Box<dyn Error + 'static>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if matches!(self.state, State::End) {
            return Poll::Ready(None);
        }

        let mut this = self.project();

        let mut data = BytesMut::new();
        if matches!(this.state, State::Start) {
            data.put_u8(b'[')
        }

        let res = ready!(this.stream.as_mut().poll_next(cx));

        match res {
            Some(Err(err)) => return Poll::Ready(Some(Err(Box::new(err)))),
            Some(Ok(item)) => {
                // first/next item
                if matches!(this.state, State::Data) {
                    data.put_u8(b',');
                }
                // serialize
                match serde_json::to_vec(&item) {
                    Ok(buffer) => data.put(Bytes::from(buffer)),
                    Err(err) => return Poll::Ready(Some(Err(Box::new(err)))),
                }
                // change state after encoding
                *this.state = State::Data;
            }
            None => {
                // no more content
                *this.state = State::End;
            }
        };

        if matches!(this.state, State::End) {
            data.put_u8(b']');
        }

        if data.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(data.into())))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use drogue_cloud_service_api::webapp as actix_web;
    use futures::{stream, TryStreamExt};

    #[tokio::test]
    async fn test_streamer_default() {
        let data: Vec<Result<_, actix_web::Error>> = vec![Ok("foo"), Ok("bar")];
        let streamer = ArrayStreamer::new(stream::iter(data));
        let outcome: Vec<Bytes> = streamer.try_collect().await.unwrap();
        let outcome: String = outcome
            .into_iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap_or_default())
            .collect();
        assert_eq!(outcome, r#"["foo","bar"]"#);
    }

    #[tokio::test]
    async fn test_streamer_empty() {
        let data: Vec<Result<String, actix_web::Error>> = vec![];
        let streamer = ArrayStreamer::new(stream::iter(data));
        let outcome: Vec<Bytes> = streamer.try_collect().await.unwrap();
        let outcome: String = outcome
            .into_iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap_or_default())
            .collect();
        assert_eq!(outcome, r#"[]"#);
    }
}
