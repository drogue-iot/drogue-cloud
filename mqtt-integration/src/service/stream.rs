use anyhow::anyhow;
use cloudevents::Data;
use drogue_cloud_event_common::stream::CustomAck;
use drogue_cloud_integration_common::{self, stream::EventStream};
use drogue_cloud_mqtt_common::mqtt::Sink;
use futures_util::StreamExt;
use ntex::util::ByteString;
use ntex_bytes::Bytes;
use ntex_mqtt::{error::SendPacketError, v3, v5};
use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QoS {
    AtLeastOnce,
    AtMostOnce,
}

impl QoS {
    async fn send_v3(&self, builder: v3::PublishBuilder) -> Result<(), SendPacketError> {
        match self {
            Self::AtMostOnce => builder.send_at_most_once(),
            Self::AtLeastOnce => builder.send_at_least_once().await,
        }
    }

    async fn send_v5(&self, builder: v5::PublishBuilder) -> Result<(), anyhow::Error> {
        match self {
            Self::AtMostOnce => Ok(builder.send_at_most_once()?),
            Self::AtLeastOnce => {
                builder
                    .send_at_least_once()
                    .await
                    .map_err(|err| anyhow!("Failed to send event: {err}"))?;
                Ok(())
            }
        }
    }
}

pub struct Stream<'s> {
    pub topic: ByteString,
    pub qos: QoS,
    pub id: Option<NonZeroU32>,
    pub event_stream: EventStream<'s, CustomAck>,
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

impl Stream<'_> {
    pub async fn run(self, mut sink: Sink) {
        let result = match (&mut sink, self.content_mode) {
            // MQTT v3.1
            (Sink::V3(sink), _) => self.run_v3(sink).await,

            // MQTT v5 in structured mode
            (Sink::V5(sink), ContentMode::Structured) => self.run_v5_structured(sink).await,

            // MQTT v5 in binary mode
            (Sink::V5(sink), ContentMode::Binary) => self.run_v5_binary(sink).await,
        };

        match result {
            Ok(()) => log::debug!("Stream processor finished"),
            Err(err) => {
                log::info!("Stream processor failed: {}", err);
                sink.close();
            }
        }
    }

    pub async fn run_v3(mut self, sink: &mut v3::MqttSink) -> Result<(), anyhow::Error> {
        while let Some(handle) = self.event_stream.next().await {
            log::debug!("Event: {:?}", handle);

            let handle = handle?;
            let event = serde_json::to_vec(handle.deref())?;
            let builder = sink.publish(self.topic.clone(), event.into());

            self.qos.send_v3(builder).await?;
            self.event_stream.ack(handle)?;

            log::debug!("Sent message - go back to sleep");
        }

        Ok(())
    }

    pub async fn run_v5_structured(mut self, sink: &mut v5::MqttSink) -> Result<(), anyhow::Error> {
        let sub_ids = self.id.map(|id| vec![id]);

        while let Some(handle) = self.event_stream.next().await {
            log::debug!("Event: {:?}", handle);

            let handle = handle?;
            let event = serde_json::to_vec(handle.deref())?;
            let builder = sink
                .publish(self.topic.clone(), event.into())
                .properties(|p| {
                    p.content_type = Some("application/cloudevents+json; charset=utf-8".into());
                    p.is_utf8_payload = Some(true);
                    p.subscription_ids = sub_ids.clone();
                });

            self.qos.send_v5(builder).await?;
            self.event_stream.ack(handle)?;

            log::debug!("Sent message - go back to sleep");
        }

        Ok(())
    }

    pub async fn run_v5_binary(mut self, sink: &mut v5::MqttSink) -> Result<(), anyhow::Error> {
        let sub_ids = self.id.map(|id| vec![id]);

        while let Some(handle) = self.event_stream.next().await {
            log::debug!("Event: {:?}", handle);

            let mut handle = handle?;
            let event = handle.deref_mut();
            let topic = self.topic.clone();

            let (content_type, _, data) = event.take_data();
            let builder = match data {
                Some(Data::Binary(data)) => sink.publish(topic, data.into()),
                Some(Data::String(data)) => sink.publish(topic, data.into()),
                Some(Data::Json(data)) => {
                    sink.publish(topic.clone(), serde_json::to_vec(&data)?.into())
                }
                None => sink.publish(topic.clone(), Bytes::new()),
            };

            // convert attributes and extensions ...

            let builder = builder.properties(|p| {
                for (k, v) in event.iter() {
                    p.user_properties.push((k.into(), v.to_string().into()));
                }
                p.content_type = content_type.map(Into::into);
                p.subscription_ids = sub_ids.clone();
            });

            // ... and send
            self.qos.send_v5(builder).await?;
            self.event_stream.ack(handle)?;

            log::debug!("Sent message - go back to sleep");
        }

        Ok(())
    }
}
