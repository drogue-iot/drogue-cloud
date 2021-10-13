use actix_http::{http::StatusCode, HttpMessage, Request};
use actix_web::{
    dev::{Service, ServiceResponse},
    test::TestRequest,
};
use chrono::Duration;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::outbox::{OutboxAccessor, OutboxEntry, PostgresOutboxAccessor},
    Client,
};
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_api::auth::user::UserInformation;
use drogue_cloud_service_common::openid::ExtendedClaims;
use futures::TryStreamExt;
use log::LevelFilter;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
    (($app:ident, $sender:ident, $outbox:ident) => $($code:tt)*) => {{
        init();

        let cli = client();
        let db = db(&cli, |pg| service::PostgresManagementServiceConfig {
            pg,
            instance: "drogue-instance".to_string(),
        })?;

        let sender = MockEventSender::new();

        let pool = db.config.pg.create_pool(tokio_postgres::NoTls)?;
        let c = pool.get().await?;
        let outbox = drogue_cloud_database_common::models::outbox::PostgresOutboxAccessor::new(&c);

        let auth = drogue_cloud_service_common::mock_auth!();

        let service = service::PostgresManagementService::new(db.config.clone(), sender.clone()).unwrap();

        let data = web::Data::new(WebData {
            authenticator: None,
            service: service.clone(),
        });

        let mut $sender = sender;
        let $outbox = outbox;

        let $app = actix_web::test::init_service(
            app!(MockEventSender, 16 * 1024, auth)
                // for the management service
                .app_data(data.clone())
                // for the admin service
                .app_data(web::Data::new(apps::WebData{
                    service: service.clone(),
                }))
                .wrap_fn(|req, srv|{
                    log::warn!("Running test-user middleware");
                    use actix_web::dev::Service;
                    use actix_web::HttpMessage;
                    {
                        let user: Option<&drogue_cloud_service_api::auth::user::UserInformation> = req.app_data();
                        if let Some(user) = user {
                            log::warn!("Replacing user with test-user: {:?}", user);
                            req.extensions_mut().insert(user.clone());
                        }
                    }
                    srv.call(req)
                })
        )
        .await;

        $($code)*;

        Ok(())
    }};
}

/// Assert if events are as expected.
///
/// This will ignore differences in the "generation", as they are not predictable.
#[allow(irrefutable_let_patterns)]
pub fn assert_events(actual: Vec<Vec<Event>>, mut expected: Vec<Event>) {
    for (n, actual) in actual.into_iter().enumerate() {
        for i in actual.iter().zip(expected.iter_mut()) {
            // this if could be reworked when we have: https://github.com/rust-lang/rust/issues/54883
            if let Event::Application {
                generation: actual_generation,
                uid: actual_uid,
                ..
            }
            | Event::Device {
                generation: actual_generation,
                uid: actual_uid,
                ..
            } = i.0
            {
                if let Event::Application {
                    generation: expected_generation,
                    uid: expected_uid,
                    ..
                }
                | Event::Device {
                    generation: expected_generation,
                    uid: expected_uid,
                    ..
                } = i.1
                {
                    *expected_generation = *actual_generation;
                    *expected_uid = actual_uid.clone();
                }
            }
        }

        assert_eq!(actual, expected, "actual[{}]", n,);
    }
}

#[allow(dead_code)]
pub async fn outbox_retrieve<'c, C>(
    outbox: &'c PostgresOutboxAccessor<'c, C>,
) -> Result<Vec<Event>, ServiceError>
where
    C: Client + 'c,
{
    let result: Vec<Event> = outbox
        .fetch_unread(Duration::zero())
        .await?
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .map(|entry| entry.into())
        .collect();

    for event in &result {
        outbox.mark_seen(OutboxEntry::from(event.clone())).await?;
    }

    Ok(result)
}

#[allow(dead_code)]
pub fn user<S: AsRef<str>>(id: S) -> UserInformation {
    let claims: ExtendedClaims = serde_json::from_value(json!({
        "sub": id.as_ref(),
        "iss": "drogue:iot:test",
        "aud": "drogue",
        "exp": 0,
        "iat": 0,
    }))
    .unwrap();

    UserInformation::Authenticated(claims.into())
}

#[allow(dead_code)]
pub async fn call_http<S, B, E>(
    app: &S,
    user: &UserInformation,
    req: actix_web::test::TestRequest,
) -> S::Response
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
{
    let req = req.to_request();
    req.extensions_mut().insert(user.clone());

    actix_web::test::call_service(app, req).await
}

#[allow(dead_code)]
pub fn assert_resources(result: Value, names: &[&str]) {
    let items = result.as_array().expect("Response must be an array");
    assert_eq!(items.len(), names.len());

    let mut actual: Vec<_> = items
        .iter()
        .filter_map(|s| s["metadata"]["name"].as_str())
        .collect();

    actual.sort();

    let mut names = Vec::from(names);
    names.sort();

    assert_eq!(actual, names);
}

#[allow(dead_code)]
pub async fn create_app<S, B, E, S1>(
    app: &S,
    user: &UserInformation,
    name: S1,
    labels: HashMap<&str, &str>,
) -> anyhow::Result<()>
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
    S1: AsRef<str>,
{
    let resp = call_http(
        app,
        user,
        TestRequest::post()
            .uri("/api/registry/v1alpha1/apps")
            .set_json(&json!({
                "metadata": {
                    "name": name.as_ref(),
                    "labels": labels,
                },
            })),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::CREATED);

    Ok(())
}

#[allow(dead_code)]
pub async fn create_device<S, B, E, S1, S2>(
    app: &S,
    user: &UserInformation,
    app_name: S1,
    name: S2,
    labels: HashMap<&str, &str>,
) -> anyhow::Result<()>
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
    S1: AsRef<str>,
    S2: AsRef<str>,
{
    let resp = call_http(
        app,
        user,
        TestRequest::post()
            .uri(&format!(
                "/api/registry/v1alpha1/apps/{}/devices",
                app_name.as_ref()
            ))
            .set_json(&json!({
                "metadata": {
                    "name": name.as_ref(),
                    "application": app_name.as_ref(),
                    "labels": labels,
                },
            })),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::CREATED);

    Ok(())
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_assert() {
        let expected = vec![Event::Application {
            instance: "instance".to_string(),
            application: "app".to_string(),
            path: ".".to_string(),
            generation: 0,
            uid: "a".to_string(),
        }];
        let actual = vec![Event::Application {
            instance: "instance".to_string(),
            application: "app".to_string(),
            path: ".".to_string(),
            generation: 12345,
            uid: "b".to_string(),
        }];
        assert_events(vec![actual], expected);
    }
}
