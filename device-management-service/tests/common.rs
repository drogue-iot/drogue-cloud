use chrono::Duration;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_database_common::models::outbox::{
    OutboxAccessor, OutboxEntry, PostgresOutboxAccessor,
};
use drogue_cloud_database_common::Client;
use drogue_cloud_registry_events::Event;
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
   (($app:ident, $sender:ident, $outbox:ident) => $($code:block)*) => {{
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
            authenticator: drogue_cloud_service_common::openid::Authenticator { client: None, scopes: "".into() },
            service: service::PostgresManagementService::new(db.config.clone(), sender.clone()).unwrap(),
        });

        let mut $sender = sender;
        let $outbox = outbox;

        let mut $app =
            actix_web::test::init_service(app!(MockEventSender, data, false, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}

/// Assert if events are as expected.
///
/// This will ignore differences in the "generation", as they are not predictable.
#[allow(irrefutable_let_patterns)]
pub fn assert_events(actual: Vec<Vec<Event>>, mut expected: Vec<Event>) {
    for actual in actual {
        for i in actual.iter().zip(expected.iter_mut()) {
            // this if could be reworked when we have: https://github.com/rust-lang/rust/issues/54883
            if let Event::Application {
                generation: actual_generation,
                ..
            }
            | Event::Device {
                generation: actual_generation,
                ..
            } = i.0
            {
                if let Event::Application {
                    generation: expected_generation,
                    ..
                }
                | Event::Device {
                    generation: expected_generation,
                    ..
                } = i.1
                {
                    // can be collapsed in the future
                    *expected_generation = *actual_generation;
                }
            }
        }

        assert_eq!(actual, expected);
    }
}

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

#[cfg(test)]
mod test {

    use super::*;

    // FIXME: needs fixing
    #[test]
    fn test1() {
        let expected = vec![Event::Application {
            instance: "instance".to_string(),
            id: "app".to_string(),
            path: ".".to_string(),
            generation: 0,
        }];
        let actual = vec![Event::Application {
            instance: "instance".to_string(),
            id: "app".to_string(),
            path: ".".to_string(),
            generation: 12345,
        }];
        assert_events(vec![actual], expected);
    }
}
