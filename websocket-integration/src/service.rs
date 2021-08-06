use anyhow::Result;

use actix::prelude::{Actor, Context, Handler, Recipient};

use std::collections::HashMap;

use crate::messages::{Disconnect, Subscribe, WsEvent};
use actix::{AsyncContext, ResponseFuture, SpawnHandle};
use drogue_cloud_integration_common::stream::{EventStream, EventStreamConfig};

use drogue_cloud_service_api::kafka::{KafkaClientConfig, KafkaConfig};

use futures::StreamExt;
use uuid::Uuid;

// Service Actor.
// Read from the kafka and forwards messages to the Web socket actors
pub struct Service {
    clients: HashMap<Uuid, Stream>,
    kafka_config: KafkaClientConfig,
}

impl Actor for Service {
    type Context = Context<Self>;
}

impl Default for Service {
    fn default() -> Service {
        Service {
            clients: HashMap::new(),
            kafka_config: KafkaClientConfig::default(),
        }
    }
}

pub struct Stream {
    application: String,
    runner: SpawnHandle,
}

/// Handle incoming messages from the WsHandler actor.
impl Handler<Subscribe> for Service {
    type Result = ResponseFuture<bool>;

    fn handle(&mut self, msg: Subscribe, ctx: &mut Context<Self>) -> Self::Result {
        let app = msg.application.clone();
        let id = msg.id;
        let addr = msg.addr.clone();
        // set up a stream
        let stream = Service::get_stream(self.kafka_config.clone(), app.clone());
        match stream {
            Ok(stream) => {
                // run the stream in a subprocess
                let fut = async move {
                    // using _ because it's a while loop so a result isn't really expected
                    let _ = Service::run_stream(stream, addr, app.clone().as_str()).await;
                };
                let fut = actix::fut::wrap_future::<_, Self>(fut);
                let run_handle = ctx.spawn(fut);

                // store the stream
                self.clients.insert(
                    id,
                    Stream {
                        application: msg.application.clone(),
                        runner: run_handle,
                    },
                );
                // subscribe was successful, respond true to the WsHandler
                Box::pin(async move { true })
            }
            Err(_) => Box::pin(async move { false }),
        }
    }
}

impl Handler<Disconnect> for Service {
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

impl Service {
    fn get_stream(
        kafka_client_config: KafkaClientConfig,
        application: String,
    ) -> Result<EventStream<'static>> {
        // extract the shared named, which we use as kafka consumer group id
        // TODO get group id
        let group_id = None;

        // log the request
        log::debug!(
            "Request to attach to app stream: {} (group: {:?})",
            application,
            group_id
        );

        // create stream
        let stream = EventStream::new(EventStreamConfig {
            kafka: KafkaConfig {
                client: kafka_client_config,
                topic: application.clone(),
            },
            consumer_group: group_id,
        })
        .map_err(|err| {
            log::info!("Failed to subscribe to Kafka topic: {}", err);
            err
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

        Ok(())
    }
}

// todo add tests
