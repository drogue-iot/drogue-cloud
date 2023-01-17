use super::publish_handler;
use super::App;

use coap_lite::{CoapRequest, Packet};
use drogue_cloud_endpoint_common::psk::PskIdentityRetriever;
use drogue_cloud_endpoint_common::x509::ClientCertificateRetriever;
use tokio_dtls_stream_sink::Session as DtlsSession;

use std::net::SocketAddr;
use tokio::select;
use tokio::time::Instant;

/// Represents a CoAP request/response exchange. Works with any transport.
pub struct Session {
    expiry: Instant,
    peer: DtlsSession,
    app: App,
}

impl Session {
    pub fn new(expiry: Instant, peer: DtlsSession, app: App) -> Self {
        Self { expiry, peer, app }
    }

    pub async fn run(&mut self) {
        let timeout = self.expiry - Instant::now();
        select! {
            _ = self.process() => {
                log::info!("Processing stopped, stopping");
            }
            _ = tokio::time::sleep(timeout) => {
                log::info!("Session expired, stopping");
            }
        }
    }

    // Processing loop for a session
    async fn process(&mut self) {
        loop {
            let mut payload = [0; 2048];
            match self.peer.read(&mut payload[..]).await {
                Ok(len) => match Packet::from_bytes(&payload[..len]) {
                    Ok(packet) => {
                        let request: CoapRequest<SocketAddr> =
                            CoapRequest::from_packet(packet, self.peer.peer());
                        let response = publish_handler(
                            request,
                            self.peer.client_certs(),
                            self.peer.verified_identity(),
                            self.app.clone(),
                        )
                        .await;
                        if let Some(response) = response {
                            log::debug!("Returning response: {:?}", response);
                            match response.message.to_bytes() {
                                Ok(packet) => match self.peer.write(&packet[..]).await {
                                    Ok(_) => {}
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
                Err(e) => {
                    log::info!("Processing stopped: {:?}", e);
                    break;
                }
            }
        }
    }
}
