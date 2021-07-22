use actix_web::{http::StatusCode, HttpResponse};
use bytes::{BufMut, Bytes, BytesMut};
use futures::{
    task::{Context, Poll},
    {ready, Stream},
};
use pin_project::pin_project;
use serde::Serialize;
use std::{
    fmt::{Debug, Display, Formatter},
    pin::Pin,
};

/// The internal state of the stream
enum State {
    /// Before the first item
    Start,
    /// In the middle of processing
    Data,
    /// After the last item
    End,
}

#[derive(Debug)]
pub enum ArrayStreamerError<E>
where
    E: Debug + Display,
{
    Source(E),
    Serializer(serde_json::Error),
}

impl<E> Display for ArrayStreamerError<E>
where
    E: Debug + Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Source(err) => write!(f, "Source error: {}", err),
            Self::Serializer(err) => write!(f, "Serializer error: {}", err),
        }
    }
}

impl<E> actix_web::ResponseError for ArrayStreamerError<E>
where
    E: Debug + Display + actix_web::ResponseError,
{
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Source(err) => err.status_code(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            Self::Source(err) => err.error_response(),
            Self::Serializer(err) => HttpResponse::InternalServerError().body(err.to_string()),
        }
    }
}

impl<E: Debug + Display> std::error::Error for ArrayStreamerError<E> {}

#[pin_project]
pub struct ArrayStreamer<S, T, E>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Debug + Display,
{
    #[pin]
    stream: S,
    state: State,
}

impl<S, T, E> ArrayStreamer<S, T, E>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Debug + Display,
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
    E: Debug + Display,
{
    type Item = Result<Bytes, ArrayStreamerError<E>>;

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
            Some(Err(err)) => return Poll::Ready(Some(Err(ArrayStreamerError::Source(err)))),
            Some(Ok(item)) => {
                // first/next item
                if matches!(this.state, State::Data) {
                    data.put_u8(b',');
                }
                // serialize
                match serde_json::to_vec(&item) {
                    Ok(buffer) => data.put(Bytes::from(buffer)),
                    Err(err) => return Poll::Ready(Some(Err(ArrayStreamerError::Serializer(err)))),
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
    use futures::{stream, TryStreamExt};

    #[tokio::test]
    async fn test_streamer_default() {
        let data: Vec<Result<_, String>> = vec![Ok("foo"), Ok("bar")];
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
        let data: Vec<Result<String, String>> = vec![];
        let streamer = ArrayStreamer::new(stream::iter(data));
        let outcome: Vec<Bytes> = streamer.try_collect().await.unwrap();
        let outcome: String = outcome
            .into_iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap_or_default())
            .collect();
        assert_eq!(outcome, r#"[]"#);
    }
}
