use super::publish_handler;
use super::App;

use bytes::{Bytes, BytesMut};
use coap_lite::{CoapRequest, Packet};
use core::pin::Pin;
use drogue_cloud_endpoint_common::x509::ClientCertificateChain;

use futures::{Sink, SinkExt, Stream, StreamExt};
use std::net::SocketAddr;
use tokio::select;
use tokio::time::Instant;

pub type SessionSink = dyn Sink<Bytes, Error = std::io::Error> + Send;
pub type SessionStream = dyn Stream<Item = Result<BytesMut, std::io::Error>> + Send;

/// Represents a CoAP request/response exchange. Works with any transport.
pub struct Session {
    peer: SocketAddr,
    expiry: Instant,
    outbound: Pin<Box<SessionSink>>,
    inbound: Pin<Box<SessionStream>>,
    app: App,
    certs: Option<ClientCertificateChain>,
}

impl Session {
    pub fn new(
        peer: SocketAddr,
        expiry: Instant,
        outbound: Pin<Box<SessionSink>>,
        inbound: Pin<Box<SessionStream>>,
        app: App,
        certs: Option<ClientCertificateChain>,
    ) -> Self {
        Self {
            expiry,
            peer,
            outbound,
            inbound,
            app,
            certs,
        }
    }

    pub async fn run(&mut self) {
        let timeout = self.expiry - Instant::now();
        select! {
            _ = self.process() => {
                log::trace!("Processing stopped, exiting");
            }
            _ = tokio::time::sleep(timeout) => {
                log::trace!("Session expired, stopping");
            }
        }
    }

    // Processing loop for a session
    async fn process(&mut self) {
        loop {
            match self.inbound.next().await {
                Some(Ok(payload)) => match Packet::from_bytes(&payload) {
                    Ok(packet) => {
                        let request = CoapRequest::from_packet(packet, self.peer);
                        let response =
                            publish_handler(request, self.certs.clone(), self.app.clone()).await;
                        if let Some(response) = response {
                            log::debug!("Returning response: {:?}", response);
                            match response.message.to_bytes() {
                                Ok(packet) => match self
                                    .outbound
                                    .send(Bytes::copy_from_slice(&packet[..]))
                                    .await
                                {
                                    Ok(_) => log::trace!("Response sent"),
                                    Err(e) => log::warn!("Error sending response: {:?}", e),
                                },
                                Err(e) => {
                                    log::warn!("Error encoding response packet: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Error decoding request packet: {:?}", e);
                    }
                },
                Some(Err(e)) => {
                    log::warn!("Processing error: {:?}", e);
                }
                None => {
                    log::debug!("Channel was closed, stopping processing");
                    break;
                }
            }
        }
    }
}
