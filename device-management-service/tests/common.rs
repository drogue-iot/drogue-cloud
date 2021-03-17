use actix_http::{HttpMessage, Request};
use actix_web::dev::{Service, ServiceResponse};
use chrono::Duration;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::outbox::{OutboxAccessor, OutboxEntry, PostgresOutboxAccessor},
    Client,
};
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_common::auth::UserInformation;
use futures::TryStreamExt;
use log::LevelFilter;

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

        let data = web::Data::new(WebData {
            authenticator: None,
            service: service::PostgresManagementService::new(db.config.clone(), sender.clone())
                .unwrap(),
        });

        let auth = drogue_cloud_service_common::mock_auth!();

        let mut $sender = sender;
        let $outbox = outbox;

        let $app =
            actix_web::test::init_service(app!(MockEventSender, data, false, 16 * 1024, auth)
                .wrap_fn(|req, srv|{
                    log::warn!("Running test-user middleware");
                    use actix_web::dev::Service;
                    use actix_web::HttpMessage;
                    {
                        let user : Option<&drogue_cloud_service_common::auth::UserInformation> = req.app_data();
                        if let Some(user) = user {
                            log::warn!("Replacing user with test-user: {:?}", user);
                            req.extensions_mut().insert(user.clone());
                        }
                    }
                    srv.call(req)
                }))
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

pub fn user(id: &str) -> UserInformation {
    use serde_json::json;

    let claims = serde_json::from_value(json!({
        "sub": id,
        "iss": "drogue:iot:test",
        "aud": "drogue",
        "exp": 0,
        "iat": 0,
    }))
    .unwrap();

    UserInformation::Authenticated(claims)
}

pub async fn call_http<S, B, E>(
    app: &S,
    user: UserInformation,
    req: actix_web::test::TestRequest,
) -> S::Response
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
{
    let req = req.to_request();
    req.extensions_mut().insert(user);

    actix_web::test::call_service(app, req).await
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
