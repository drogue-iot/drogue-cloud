use std::pin::Pin;

use actix_web::{
    get,
    http::StatusCode,
    web::{Bytes, BytesMut},
    HttpResponse, Responder,
};

use cloudevents_sdk_rdkafka::MessageExt;

use futures::{
    task::{Context, Poll},
    Stream, StreamExt,
};

use actix::clock::{interval_at, Instant};
use futures::stream::select;

use owning_ref::OwningHandle;

use rdkafka::{
    config::{ClientConfig, RDKafkaLogLevel},
    consumer::{stream_consumer::StreamConsumer, CommitMode, Consumer, DefaultConsumerContext},
};

use rdkafka::message::BorrowedMessage;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SpyConfig {
    bootstrap_servers: String,
    topic: String,
}

pub struct MessageSpy {
    upstream: OwningHandle<
        Box<StreamConsumer>,
        Box<rdkafka::consumer::MessageStream<'static, DefaultConsumerContext>>,
    >,
}

impl MessageSpy {
    pub fn new(consumer: StreamConsumer<DefaultConsumerContext>) -> Self {
        MessageSpy {
            upstream: OwningHandle::new_with_fn(Box::new(consumer), |c| {
                Box::new(unsafe { &*c }.start())
            }),
        }
    }

    fn to_event(&self, msg: &BorrowedMessage) -> Result<String, anyhow::Error> {
        let event = msg
            .to_event()
            .map_err(|err| anyhow::anyhow!("Failed to convert to event: {}", err.to_string()))?;
        Ok(serde_json::to_string(&event)?)
    }

    fn to_frame(&self, event: String) -> Bytes {
        let mut r = BytesMut::new();

        r.extend(b"data: ");
        r.extend(event.as_bytes());
        r.extend(b"\n\n");

        r.freeze()
    }
}

impl Stream for MessageSpy {
    type Item = Result<Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.upstream.poll_next_unpin(cx).map(|next| {
            log::info!("Event: {:?}", next);

            match next {
                None => None,
                Some(Err(e)) => Some(Err(actix_web::error::InternalError::new(
                    e,
                    StatusCode::from_u16(500).unwrap(),
                )
                .into())),
                Some(Ok(msg)) => {
                    let event = self
                        .upstream
                        .as_owner()
                        .commit_message(&msg, CommitMode::Async)
                        .map_err(|err| err.into())
                        .and_then(|_| self.to_event(&msg))
                        .map(|event| self.to_frame(event))
                        .map_err(|e| {
                            actix_web::error::InternalError::new(
                                e,
                                StatusCode::from_u16(500).unwrap(),
                            )
                            .into()
                        });

                    Some(event)
                }
            }
        })
    }
}

#[get("/spy")]
pub async fn stream_events() -> impl Responder {
    let group_id = Uuid::new_v4().to_string();

    let cfg = SpyConfig {
        bootstrap_servers: "kafka-eventing-kafka-bootstrap.knative-eventing.svc:9092".into(),
        topic: "knative-messaging-kafka.drogue-iot.iot-channel".into(),
    };

    log::info!("Config: {:?}", cfg);

    let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
        .set("group.id", &group_id)
        .set("bootstrap.servers", &cfg.bootstrap_servers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        //.set("statistics.interval.ms", "30000")
        //.set("auto.offset.reset", "smallest")
        .set_log_level(RDKafkaLogLevel::Debug)
        .create_with_context(DefaultConsumerContext)
        .expect("Consumer creation failed");

    log::info!("Created consumer");

    consumer
        .subscribe(&[cfg.topic.as_str()])
        .expect("Can't subscribe to the specified topics");

    log::info!("Subscribed");

    let stream = MessageSpy::new(consumer);
    let hb = interval_at(Instant::now(), Duration::from_secs(5))
        .map(|_| Ok(Bytes::from("event: ping\n\n")));
    let stream = select(stream, hb);

    HttpResponse::Ok()
        .header("content-type", "text/event-stream")
        .streaming(stream)
}
