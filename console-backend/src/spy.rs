use crate::Config;
use actix::clock::{interval_at, Instant};
use actix_http::http::header::ContentType;
use actix_web::{
    get,
    http::StatusCode,
    web,
    web::{Bytes, BytesMut},
    HttpResponse,
};
use cloudevents::{event::ExtensionValue, Event};
use cloudevents_sdk_rdkafka::MessageExt;
use drogue_cloud_service_api::EXT_APPLICATION;
use drogue_cloud_service_common::{error::ServiceError, openid::Authenticator};
use futures::{
    stream::select,
    task::{Context, Poll},
    Stream, StreamExt,
};
use owning_ref::OwningHandle;
use rdkafka::{
    config::{ClientConfig, RDKafkaLogLevel},
    consumer::{stream_consumer::StreamConsumer, CommitMode, Consumer, DefaultConsumerContext},
    message::BorrowedMessage,
    util::Timeout,
    TopicPartitionList,
};
use serde::Deserialize;
use std::{pin::Pin, time::Duration};
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SpyConfig {
    bootstrap_servers: String,
    topic: String,
    app_id: Option<String>,
}

pub struct MessageSpy {
    upstream: OwningHandle<
        Box<StreamConsumer>,
        Box<rdkafka::consumer::MessageStream<'static, DefaultConsumerContext>>,
    >,
    app_id: Option<String>,
}

impl MessageSpy {
    pub fn new(cfg: SpyConfig) -> Result<Self, ServiceError> {
        Self::new_with_group(cfg)
    }

    /// Create a new message spy without using group management
    ///
    /// This is currently blocked by:https://github.com/edenhill/librdkafka/issues/3261
    #[allow(dead_code)]
    fn new_without_group(cfg: SpyConfig) -> Result<Self, ServiceError> {
        let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create_with_context(DefaultConsumerContext)
            .map_err(|err| {
                log::debug!("Failed to create Kafka consumer: {}", err);
                ServiceError::ServiceUnavailable("Failed to create Kafka consumer".into())
            })?;

        log::info!("Created consumer");

        let topic = cfg.topic;

        let metadata = consumer
            .fetch_metadata(Some(&topic), Timeout::After(Duration::from_secs(10)))
            .map_err(|err| {
                log::debug!("Failed to fetch metadata: {}", err);
                ServiceError::ServiceUnavailable("Failed to fetch metadata for topic".into())
            })?;

        let partitions = metadata
            .topics()
            .iter()
            .find(|t| t.name() == topic)
            .map(|topic| topic.partitions())
            .ok_or_else(|| {
                log::debug!("Failed to find metadata for topic");
                ServiceError::ServiceUnavailable("Unable to find metadata for topic".into())
            })?;

        log::debug!("Topic has {} partitions", partitions.len());

        let mut assignment = TopicPartitionList::with_capacity(partitions.len());
        for part in partitions {
            log::debug!("Adding partition: {}", part.id());
            assignment.add_partition(&topic, part.id());
        }

        consumer.assign(&assignment).map_err(|err| {
            log::debug!("Failed to assign consumer: {}", err);
            ServiceError::ServiceUnavailable("Unable to assign consumer to topic".into())
        })?;

        log::info!("Subscribed");

        Self::wrap(cfg.app_id, consumer)
    }

    fn new_with_group(cfg: SpyConfig) -> Result<Self, ServiceError> {
        // create a random subscriber group
        let group_id = format!("anonymous.{}", Uuid::new_v4());

        let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
            .set("group.id", &group_id)
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create_with_context(DefaultConsumerContext)
            .map_err(|err| {
                log::debug!("Failed to create Kafka consumer: {}", err);
                ServiceError::ServiceUnavailable("Failed to create Kafka consumer".into())
            })?;

        log::info!("Created consumer");

        consumer.subscribe(&[&cfg.topic]).map_err(|err| {
            log::debug!("Failed to subscribe consumer: {}", err);
            ServiceError::ServiceUnavailable("Unable to subscribe consumer to topic".into())
        })?;

        log::info!("Subscribed");

        Self::wrap(cfg.app_id, consumer)
    }

    fn wrap(app_id: Option<String>, consumer: StreamConsumer) -> Result<Self, ServiceError> {
        Ok(MessageSpy {
            upstream: OwningHandle::new_with_fn(Box::new(consumer), |c| {
                Box::new(unsafe { &*c }.stream())
            }),
            app_id,
        })
    }

    /// Convert a Kafka message to an event.
    fn to_event(&self, msg: &BorrowedMessage) -> Result<Event, anyhow::Error> {
        let event = msg
            .to_event()
            .map_err(|err| anyhow::anyhow!("Failed to convert to event: {}", err.to_string()))?;
        Ok(event)
    }

    /// Test if the message/event matches an optional filter.
    fn matches(&self, event: &Event) -> bool {
        match (&self.app_id, event.extension(EXT_APPLICATION)) {
            (None, _) => true,
            (Some(app_id_1), Some(ExtensionValue::String(app_id_2))) => app_id_1 == app_id_2,
            _ => false,
        }
    }

    /// create an SSE frame from an even already in string format
    fn make_frame(event: String) -> Bytes {
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
        let next = self.upstream.poll_next_unpin(cx);

        match next {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => {
                log::debug!("Event: {:?}", next);
                match next {
                    None => Poll::Ready(None),
                    Some(Err(e)) => Poll::Ready(Some(Err(actix_web::error::InternalError::new(
                        e,
                        StatusCode::from_u16(500).unwrap(),
                    )
                    .into()))),
                    Some(Ok(msg)) => {
                        let event = self
                            .upstream
                            .as_owner()
                            .commit_message(&msg, CommitMode::Async)
                            .map_err(|err| err.into())
                            .and_then(|_| self.to_event(&msg))
                            .map(|event| match self.matches(&event) {
                                true => Some(event),
                                false => None,
                            })
                            .and_then(|event| {
                                event
                                    .map(|event| {
                                        serde_json::to_string(&event)
                                            .map_err(Into::into)
                                            .map(Self::make_frame)
                                    })
                                    .transpose()
                            })
                            .map_err(|e| {
                                log::debug!("Failed to process event: {}", e);
                                actix_web::error::InternalError::new(
                                    e,
                                    StatusCode::from_u16(500).unwrap(),
                                )
                                .into()
                            })
                            .transpose();

                        match event {
                            Some(result) => Poll::Ready(Some(result)),
                            None => Poll::Pending,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpyQuery {
    token: String,
    app: Option<String>,
}

#[get("/spy")]
pub async fn stream_events(
    authenticator: web::Data<Authenticator>,
    query: web::Query<SpyQuery>,
    config: web::Data<Config>,
) -> Result<HttpResponse, actix_web::Error> {
    authenticator
        .validate_token(query.token.clone())
        .await
        .map_err(|_| ServiceError::AuthenticationError)?;

    let cfg = SpyConfig {
        bootstrap_servers: config.kafka_boostrap_servers.clone(),
        topic: config.kafka_topic.clone(),
        app_id: query.app.as_ref().cloned(),
    };

    log::debug!("Config: {:?}", cfg);

    let stream = MessageSpy::new(cfg)?;
    let hb = IntervalStream::new(interval_at(Instant::now(), Duration::from_secs(5)))
        .map(|_| Ok(Bytes::from("event: ping\n\n")));
    let stream = select(stream, hb);

    Ok(HttpResponse::Ok()
        .append_header(ContentType(mime::TEXT_EVENT_STREAM))
        .streaming(stream))
}
