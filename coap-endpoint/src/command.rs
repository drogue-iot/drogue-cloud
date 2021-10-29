//! CoAP context command handlers
//!
//! Contains actors that handles commands for CoAP endpoint

use crate::{error::CoapEndpointError, HEADER_COMMAND};
use actix_rt::time::timeout;
use coap_lite::{CoapRequest, CoapResponse, ResponseType};
use drogue_cloud_endpoint_common::command::{CommandFilter, Commands};
use std::{net::SocketAddr, time::Duration};

pub async fn wait_for_command(
    req: CoapRequest<SocketAddr>,
    commands: Commands,
    filter: CommandFilter,
    ttd: Option<u64>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
    match ttd {
        // If command timeout > 0, subscribe to command receiver.
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(filter.clone()).await;
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                // Command is received
                Ok(Some(cmd)) => {
                    commands.unsubscribe(&filter).await;
                    log::debug!("Got command: {:?}", cmd);
                    // Construct response
                    Ok(req.response.map(|mut v| {
                        v.set_status(ResponseType::Content);
                        v.message
                            .add_option(HEADER_COMMAND, cmd.command.as_bytes().to_vec());
                        v.message.payload = cmd.payload.unwrap_or_default();
                        v
                    }))
                }
                // If time limit is reached
                _ => {
                    commands.unsubscribe(&filter).await;
                    Ok(req.response.map(|mut v| {
                        v.set_status(ResponseType::Changed);
                        v.message.payload = vec![];
                        v
                    }))
                }
            }
        }
        _ => Ok(req.response.map(|mut v| {
            v.set_status(ResponseType::Changed);
            v.message.payload = vec![];
            v
        })),
    }
}
