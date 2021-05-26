use crate::Config;
use actix::clock::{interval_at, Instant};
use actix_http::http::header::ContentType;
use actix_web::{get, web, web::Bytes, HttpResponse};
use drogue_cloud_integration_common::stream::{EventStream, EventStreamConfig, IntoSseStream};
use drogue_cloud_service_api::auth::user::{
    authz::{AuthorizationRequest, Permission},
    UserInformation,
};
use drogue_cloud_service_common::{
    client::UserAuthClient, error::ServiceError, openid::Authenticator,
};
use futures::{stream::select, StreamExt};
use openid::CustomClaims;
use serde::Deserialize;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;

#[derive(Deserialize, Debug, Clone)]
pub struct SpyQuery {
    token: String,
    app: String,
}

#[get("/spy")]
pub async fn stream_events(
    authenticator: web::Data<Authenticator>,
    query: web::Query<SpyQuery>,
    config: web::Data<Config>,
    user_auth: Option<web::Data<UserAuthClient>>,
) -> Result<HttpResponse, actix_web::Error> {
    if let Some(user_auth) = user_auth {
        let user = authenticator
            .validate_token(query.token.clone())
            .await
            .map_err(|_| ServiceError::AuthenticationError)?;

        let user_id = user.standard_claims().sub.clone();
        let roles = UserInformation::Authenticated(user.into())
            .roles()
            .iter()
            .map(ToString::to_string)
            .collect();

        user_auth
            .authorize(
                AuthorizationRequest {
                    application: query.app.clone(),
                    permission: Permission::Read,
                    user_id,
                    roles,
                },
                Default::default(),
            )
            .await
            .map_err(|err| ServiceError::InternalError {
                message: format!("Authorization failed: {}", err),
            })?
            .outcome
            .ensure(|| ServiceError::AuthenticationError)?
    }

    let cfg = EventStreamConfig {
        bootstrap_servers: config.kafka_boostrap_servers.clone(),
        topic: config.kafka_topic.clone(),
        app: query.app.clone(),
        consumer_group: None,
    };

    log::debug!("Config: {:?}", cfg);

    let stream = EventStream::new(cfg).map_err(|err| {
        ServiceError::ServiceUnavailable(format!("Failed to connect to Kafka: {}", err))
    })?;

    let hb = IntervalStream::new(interval_at(Instant::now(), Duration::from_secs(5)))
        .map(|_| Ok(Bytes::from("event: ping\n\n")));
    let stream = select(stream.into_sse_stream(), hb);

    Ok(HttpResponse::Ok()
        .append_header(ContentType(mime::TEXT_EVENT_STREAM))
        .streaming(stream))
}
