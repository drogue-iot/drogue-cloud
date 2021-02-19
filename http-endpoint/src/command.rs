//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix_web::web;
use actix_web::{http, HttpResponse};
use drogue_cloud_endpoint_common::commands::Commands;
use drogue_cloud_endpoint_common::error::HttpEndpointError;
use drogue_cloud_service_common::Id;

use actix_rt::time::timeout;
use std::time::Duration;

pub async fn wait_for_command(
    commands: web::Data<Commands>,
    id: Id,
    ttd: Option<u64>,
) -> Result<HttpResponse, HttpEndpointError> {
    match ttd {
        Some(ttd) if ttd > 0 => {
            let mut receiver = commands.subscribe(id.clone());
            match timeout(Duration::from_secs(ttd), receiver.recv()).await {
                Ok(command) => {
                    commands.unsubscribe(id.clone());
                    Ok(HttpResponse::Ok().body(command.unwrap()))
                }
                _ => {
                    commands.unsubscribe(id.clone());
                    Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish())
                }
            }
        }
        _ => Ok(HttpResponse::build(http::StatusCode::ACCEPTED).finish()),
    }
}
