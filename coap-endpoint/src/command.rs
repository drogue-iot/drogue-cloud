//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use crate::error::CoapEndpointError;
use coap_lite::{CoapOption, CoapRequest, CoapResponse, ResponseType};
use drogue_cloud_endpoint_common::commands::Commands;
use drogue_cloud_service_common::Id;

use actix_rt::time::timeout;
use std::collections::LinkedList;
use std::net::SocketAddr;
use std::time::Duration;

const HEADER_COMMAND: CoapOption = CoapOption::Unknown(4210);

pub async fn wait_for_command(
    req: CoapRequest<SocketAddr>,
    commands: Commands,
    id: Id,
    ttd: Option<u64>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(id.clone());
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(Some(cmd)) => {
                    commands.unsubscribe(id.clone());
                    log::debug!("Got command: {:?}", cmd);
                    Ok(req.response.and_then(|mut v| {
                        v.set_status(ResponseType::Content);
                        let mut command_value = LinkedList::new();
                        command_value.push_back(cmd.command.as_bytes().to_vec());
                        v.message.set_option(HEADER_COMMAND, command_value);
                        v.message.payload = cmd.payload.unwrap_or_default().as_bytes().to_vec();
                        Some(v)
                    }))
                }
                _ => {
                    commands.unsubscribe(id.clone());
                    Ok(req.response.and_then(|mut v| {
                        v.set_status(ResponseType::Changed);
                        Some(v)
                    }))
                }
            }
        }
        _ => Ok(req.response.and_then(|mut v| {
            v.set_status(ResponseType::Changed);
            Some(v)
        })),
    }
}
