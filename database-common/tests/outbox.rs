use chrono::Duration;
use drogue_cloud_database_common::models::outbox::{
    OutboxAccessor, OutboxEntry, PostgresOutboxAccessor,
};
use drogue_cloud_database_common::utils::millis_since_epoch;
use drogue_cloud_test_common::{client, db};
use futures::TryStreamExt;
use log::LevelFilter;
use tokio_postgres::NoTls;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[tokio::test]
async fn test_outbox() -> anyhow::Result<()> {
    init();

    let cli = client();
    let db = db(&cli, |pg| pg)?;

    let pool = db.config.create_pool(NoTls)?;
    let c = pool.get().await?;

    let outbox = PostgresOutboxAccessor::new(&c);

    // create a first entry

    let ms1 = millis_since_epoch();

    outbox
        .create(OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms1,
        })
        .await?;

    // fetch

    let entries: Vec<_> = outbox
        .fetch_unread(Duration::zero())
        .await?
        .try_collect()
        .await?;

    // there should be one entry now

    assert_eq!(
        entries,
        vec![OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms1,
        }]
    );

    // update the same entry

    let ms2 = millis_since_epoch();

    outbox
        .create(OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms2,
        })
        .await?;

    // fetch

    let entries: Vec<_> = outbox
        .fetch_unread(Duration::zero())
        .await?
        .try_collect()
        .await?;

    // there still should be only one entry

    assert_eq!(
        entries,
        vec![OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms2,
        }]
    );

    // mark seen - ms1

    outbox
        .mark_seen(OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms1,
        })
        .await?;

    // fetch

    let entries: Vec<_> = outbox
        .fetch_unread(Duration::zero())
        .await?
        .try_collect()
        .await?;

    // there still should be one entry, as the timestamp was older

    assert_eq!(
        entries,
        vec![OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms2,
        }]
    );

    // mark seen - ms2

    outbox
        .mark_seen(OutboxEntry {
            instance_id: "instance1".to_string(),
            app_id: "app1".to_string(),
            device_id: None,
            path: ".path".to_string(),
            generation: ms2,
        })
        .await?;

    // fetch

    let entries: Vec<_> = outbox
        .fetch_unread(Duration::zero())
        .await?
        .try_collect()
        .await?;

    // now there should be no entry

    assert_eq!(entries, vec![]);

    Ok(())
}
