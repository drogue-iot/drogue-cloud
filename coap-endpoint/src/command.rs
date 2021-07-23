//! CoAP context command handlers
//!
//! Contains actors that handles commands for CoAP endpoint

use crate::{error::CoapEndpointError, HEADER_COMMAND};
use actix_rt::time::timeout;
use coap_lite::{CoapRequest, CoapResponse, ResponseType};
use drogue_cloud_endpoint_common::command::Commands;
use drogue_cloud_service_common::Id;
use std::collections::LinkedList;
use std::net::SocketAddr;
use std::time::Duration;

pub async fn wait_for_command(
    req: CoapRequest<SocketAddr>,
    commands: Commands,
    id: Id,
    ttd: Option<u64>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(id.clone()).await;
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(Some(cmd)) => {
                    commands.unsubscribe(&id).await;
                    log::debug!("Got command: {:?}", cmd);
                    Ok(req.response.map(|mut v| {
                        v.set_status(ResponseType::Content);
                        let mut command_value = LinkedList::new();
                        command_value.push_back(cmd.command.as_bytes().to_vec());
                        v.message.set_option(HEADER_COMMAND, command_value);
                        v.message.payload = cmd.payload.unwrap_or_default();
                        v
                    }))
                }
                _ => {
                    commands.unsubscribe(&id).await;
                    Ok(req.response.map(|mut v| {
                        v.set_status(ResponseType::Changed);
                        v
                    }))
                }
            }
        }
        _ => Ok(req.response.map(|mut v| {
            v.set_status(ResponseType::Changed);
            v
        })),
    }
}
