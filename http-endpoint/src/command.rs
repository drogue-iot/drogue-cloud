//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix_rt::time::timeout;
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands, Subscription},
    error::HttpEndpointError,
};
use drogue_cloud_service_api::webapp::{http, web, HttpResponse};
use std::time::Duration;
use tracing::instrument;

const HEADER_COMMAND: &str = "command";

#[instrument(skip(commands))]
pub async fn wait_for_command(
    commands: web::Data<Commands>,
    filter: CommandFilter,
    ttd: Option<u64>,
) -> Result<HttpResponse, HttpEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let Subscription {
                mut receiver,
                handle,
            } = commands.subscribe(filter).await;
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(Some(cmd)) => {
                    commands.unsubscribe(handle).await;
                    Ok(HttpResponse::Ok()
                        .insert_header((HEADER_COMMAND, cmd.command))
                        .body(cmd.payload.unwrap_or_default()))
                }
                _ => {
                    commands.unsubscribe(handle).await;
                    Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish())
                }
            }
        }
        _ => Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish()),
    }
}
