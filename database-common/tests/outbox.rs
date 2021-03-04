use chrono::Duration;
use drogue_cloud_database_common::{
    models::outbox::{OutboxAccessor, OutboxEntry, PostgresOutboxAccessor},
    utils::millis_since_epoch,
    Client,
};
use drogue_cloud_test_common::{client, db};
use futures::TryStreamExt;
use log::LevelFilter;
use serial_test::serial;
use tokio_postgres::NoTls;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[tokio::test]
#[serial]
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
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
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
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
            path: ".path".to_string(),
            generation: ms1,
        }]
    );

    // update the same entry

    let ms2 = millis_since_epoch();

    outbox
        .create(OutboxEntry {
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
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
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
            path: ".path".to_string(),
            generation: ms2,
        }]
    );

    // mark seen - ms1

    outbox
        .mark_seen(OutboxEntry {
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
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
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
            path: ".path".to_string(),
            generation: ms2,
        }]
    );

    // mark seen - ms2

    outbox
        .mark_seen(OutboxEntry {
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "a".to_string(),
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

struct CreateApp {
    app: String,
    uid: String,
    path: String,
    generation: u64,
}

impl CreateApp {
    fn new(app: &str, uid: &str, path: &str, generation: u64) -> Self {
        Self {
            app: app.to_string(),
            uid: uid.to_string(),
            path: path.to_string(),
            generation,
        }
    }

    async fn run<C: Client>(self, outbox: &PostgresOutboxAccessor<'_, C>) -> anyhow::Result<()> {
        outbox
            .create(OutboxEntry {
                instance: "instance1".to_string(),
                app: self.app,
                device: None,
                uid: self.uid,
                path: self.path,
                generation: self.generation,
            })
            .await?;

        Ok(())
    }
}

#[tokio::test]
#[serial]
async fn test_recreate_resource() -> anyhow::Result<()> {
    init();

    let cli = client();
    let db = db(&cli, |pg| pg)?;

    let pool = db.config.create_pool(NoTls)?;
    let c = pool.get().await?;

    let outbox = PostgresOutboxAccessor::new(&c);

    // create a flow of events

    for i in vec![
        CreateApp::new("app1", "a", ".path", 1),
        CreateApp::new("app1", "a", ".path", 2),
        CreateApp::new("app1", "a", ".path", 3),
        CreateApp::new("app1", "b", ".path", 1),
        CreateApp::new("app1", "b", ".path", 2),
    ] {
        i.run(&outbox).await?;
    }

    // now check

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
            instance: "instance1".to_string(),
            app: "app1".to_string(),
            device: None,
            uid: "b".to_string(),
            path: ".path".to_string(),
            generation: 2,
        }]
    );

    Ok(())
}
