//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix_rt::time::timeout;
use actix_web::{http, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands},
    error::HttpEndpointError,
};
use std::time::Duration;

const HEADER_COMMAND: &str = "command";

pub async fn wait_for_command(
    commands: web::Data<Commands>,
    filter: CommandFilter,
    ttd: Option<u64>,
) -> Result<HttpResponse, HttpEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(filter.clone()).await;
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(Some(cmd)) => {
                    commands.unsubscribe(&filter).await;
                    Ok(HttpResponse::Ok()
                        .insert_header((HEADER_COMMAND, cmd.command))
                        .body(cmd.payload.unwrap_or_default()))
                }
                _ => {
                    commands.unsubscribe(&filter).await;
                    Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish())
                }
            }
        }
        _ => Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish()),
    }
}
