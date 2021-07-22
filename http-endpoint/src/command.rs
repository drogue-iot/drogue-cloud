//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix_rt::time::timeout;
use actix_web::{http, web, HttpResponse};
use drogue_cloud_endpoint_common::{command::Commands, error::HttpEndpointError};
use drogue_cloud_service_common::Id;
use std::time::Duration;

const HEADER_COMMAND: &str = "command";

pub async fn wait_for_command(
    commands: web::Data<Commands>,
    id: Id,
    ttd: Option<u64>,
) -> Result<HttpResponse, HttpEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(id.clone()).await;
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(Some(cmd)) => {
                    commands.unsubscribe(&id).await;
                    Ok(HttpResponse::Ok()
                        .insert_header((HEADER_COMMAND, cmd.command))
                        .body(cmd.payload.unwrap_or_default()))
                }
                _ => {
                    commands.unsubscribe(&id).await;
                    Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish())
                }
            }
        }
        _ => Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish()),
    }
}
