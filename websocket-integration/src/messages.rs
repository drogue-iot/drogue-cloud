use actix::prelude::{Message, Recipient};
use uuid::Uuid;

// Service sends the kafka events in this message to WSHandler
#[derive(Message)]
#[rtype(result = "()")]
pub struct WsEvent(pub String);

// WsHandler sends this to service to subscribe to the stream
#[derive(Message)]
#[rtype(result = "bool")]
pub struct Subscribe {
    pub addr: Recipient<WsEvent>,
    pub application: String,
    pub id: Uuid,
}

// WsHandler sends this to the service to disconnect from the stream
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: Uuid,
}
