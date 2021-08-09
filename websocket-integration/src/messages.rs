use actix::prelude::{Message, Recipient};
use drogue_cloud_integration_common::stream::EventStream;
use drogue_cloud_service_common::error::ServiceError;
use uuid::Uuid;

// Service sends the kafka events in this message to WSHandler
#[derive(Message)]
#[rtype(result = "()")]
pub struct WsEvent(pub String);

// WsHandler sends this to service to subscribe to the stream
#[derive(Message)]
#[rtype(result = "()")]
pub struct Subscribe {
    pub addr: Recipient<WsEvent>,
    pub err_addr: Recipient<StreamError>,
    pub application: String,
    pub id: Uuid,
}

// WsHandler sends this to the service to disconnect from the stream
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: Uuid,
}

// Service sends this to itself to run the stream
#[derive(Message)]
#[rtype(result = "()")]
pub struct RunStream<'s> {
    pub sub: Subscribe,
    pub stream: EventStream<'s>,
}

// Service sends this to WSHandler if an error happens while subscribing or running the stream
#[derive(Message)]
#[rtype(result = "()")]
pub struct StreamError {
    pub error: ServiceError,
    pub id: Uuid,
}
