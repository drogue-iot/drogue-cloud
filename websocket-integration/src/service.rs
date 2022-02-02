use crate::messages::{Disconnect, StreamError, Subscribe, WsEvent};
use actix::{prelude::*, AsyncContext, SpawnHandle, WrapFuture};
use anyhow::{anyhow, Result};
use drogue_client::{openid::TokenProvider, registry::v1::Client};
use drogue_cloud_integration_common::stream::{EventStream, EventStreamConfig};
use drogue_cloud_service_api::kafka::{KafkaClientConfig, KafkaConfigExt, KafkaEventType};
use drogue_cloud_service_common::error::ServiceError;
use futures::StreamExt;
use std::collections::HashMap;
use uuid::Uuid;

// Service Actor.
// Read from the kafka and forwards messages to the Web socket actors
pub struct Service<TP: TokenProvider> {
    pub clients: HashMap<Uuid, Stream>,
    pub kafka_config: KafkaClientConfig,
    pub registry: Client<TP>,
}

impl<TP: TokenProvider + Unpin + 'static> Actor for Service<TP> {
    type Context = Context<Self>;
}

pub struct Stream {
    application: String,
    runner: SpawnHandle,
    err_addr: Recipient<StreamError>,
}

/// Handle subscribe messages from the WsHandler actor.
impl<TP: TokenProvider + Unpin + 'static> Handler<Subscribe> for Service<TP> {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, ctx: &mut Context<Self>) -> Self::Result {
        let app = msg.application.clone();
        let addr = msg.addr.clone();
        let registry_client = self.registry.clone();
        let kafka = self.kafka_config.clone();
        let consumer_group = msg.consumer_group.clone();

        let fut = async move {
            // set up a stream
            let stream =
                Service::get_stream(registry_client, &kafka, app.clone(), consumer_group).await;
            // run the stream
            let _ = match stream {
                Ok(s) => Service::<TP>::run_stream(s, addr.clone(), app.clone().as_str()).await,
                Err(s) => Err(anyhow!(s)),
            };
        }
        .into_actor(self);
        let fut = fut.map(move |_, _, ctx| {
            // if run_stream return, it means that something went wrong
            ctx.notify(StreamError {
                error: ServiceError::InternalError(String::from("Stream error")),
                id: msg.id,
            });
        });

        // spawn the future in a different thread
        let run_handle = ctx.spawn(fut);

        // store the stream
        self.clients.insert(
            msg.id,
            Stream {
                application: msg.application.clone(),
                runner: run_handle,
                err_addr: msg.err_addr,
            },
        );
    }
}

impl<TP: TokenProvider + Unpin + 'static> Handler<Disconnect> for Service<TP> {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, ctx: &mut Context<Self>) {
        let stream = self.clients.remove(&msg.id);
        match stream {
            Some(s) => {
                log::info!(
                    "Disconnect message. Dropping stream for client [id: {}, app:{}]",
                    msg.id,
                    s.application
                );
                ctx.cancel_future(s.runner);
            }
            None => {
                log::warn!("Received disconnect message for client [{}] but no stream was registered for it.", msg.id)
            }
        };
    }
}

// if there is an error with the stream, notify the WsClient and release the stream handle
impl<TP: TokenProvider + Unpin + 'static> Handler<StreamError> for Service<TP> {
    type Result = ();

    fn handle(&mut self, msg: StreamError, ctx: &mut Context<Self>) {
        let stream = self.clients.remove(&msg.id);
        match stream {
            Some(s) => {
                log::info!(
                    "Dropping stream for client [id: {}, app:{}] due to stream error: {}",
                    msg.id,
                    s.application,
                    msg.error
                );
                ctx.cancel_future(s.runner);

                let _ = s.err_addr.do_send(msg);
            }
            None => {
                log::warn!("Stream Error, but no client registered with it.")
            }
        };
    }
}

impl<TP: TokenProvider> Service<TP> {
    async fn get_stream(
        registry: Client<TP>,
        kafka_config: &KafkaClientConfig,
        application: String,
        group_id: Option<String>,
    ) -> Result<EventStream<'static>, ServiceError> {
        // log the request
        log::debug!(
            "Request to attach to app stream: {} (group: {:?})",
            application,
            group_id
        );

        let app_res = registry
            .get_app(application.clone())
            .await
            .map_err(|_| ServiceError::InternalError(String::from("Request to registry error")))?
            .ok_or_else(|| ServiceError::InternalError(String::from("Cannot find application")))?;

        // create stream
        let stream = EventStream::new(EventStreamConfig {
            kafka: app_res.kafka_config(KafkaEventType::Events, kafka_config)?,
            consumer_group: group_id,
        })
        .map_err(|err| {
            log::info!("Failed to subscribe to Kafka topic: {}", err);
            ServiceError::InternalError("Failed to subscribe to Kafka topic".to_string())
        })?;

        // we started the stream, return it ...
        log::info!("Subscribed to Kafka topic: {}", &application);
        Ok(stream)
    }

    async fn run_stream(
        mut stream: EventStream<'_>,
        recipient: Recipient<WsEvent>,
        application: &str,
    ) -> Result<(), anyhow::Error> {
        log::debug!("Running stream {:?}", application);

        // run event stream
        while let Some(event) = stream.next().await {
            log::debug!("Topic: {} - Event: {:?}", application, event);

            // Covert the event to a JSON string
            let event = serde_json::to_string(&event?)?;
            // Send the event as an Actor message. We don't really care if it fails
            let _ = recipient.do_send(WsEvent(event.to_string()));

            log::debug!("Sent message - go back to sleep");
        }

        Err(anyhow!("Stream Error"))
    }
}

// todo add tests
