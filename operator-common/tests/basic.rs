mod common;

use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_operator_common::controller::base::queue::{
    WorkQueueHandler, WorkQueueReader, WorkQueueReaderOptions, WorkQueueWriter,
};
use drogue_cloud_test_common::{client, db};
use futures::lock::Mutex;
use serial_test::serial;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio_postgres::NoTls;

#[derive(Clone)]
struct MockHandler {
    events: Arc<Mutex<Vec<String>>>,
}

impl MockHandler {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(vec![])),
        }
    }

    pub async fn retrieve(&self) -> Vec<String> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl WorkQueueHandler<String> for MockHandler {
    async fn handle(&self, key: String) -> Result<Option<(String, Duration)>, ()> {
        self.events.lock().await.push(key);
        Ok(None)
    }
}

async fn test_with<F, Fut>(f: F) -> Vec<String>
where
    F: FnOnce(WorkQueueWriter) -> Fut,
    Fut: Future<Output = ()>,
{
    let cli = client();
    let db = db(&cli, |pg| pg).unwrap();

    let pool: Pool = db.config.create_pool(NoTls).unwrap();

    let handler = MockHandler::new();

    let writer = WorkQueueWriter::new(pool.clone(), "drogue".into(), "foo".into());
    let _reader = WorkQueueReader::with_options(
        pool,
        "drogue".into(),
        "foo".into(),
        handler.clone(),
        WorkQueueReaderOptions {
            delay: Duration::from_millis(250),
        },
    );

    f(writer).await;

    handler.retrieve().await
}

/// Test a zero delay (just re-queue) event
#[actix_rt::test]
#[serial]
async fn test_inbox_zero() {
    common::init();

    let events = test_with(|writer| async move {
        writer.add("A".to_string(), Duration::ZERO).await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    })
    .await;

    assert_eq!(vec!["A".to_string()], events);
}

/// Test to shorten the delay with a second call
#[actix_rt::test]
#[serial]
async fn test_inbox_shorten() {
    common::init();

    let events = test_with(|writer| async move {
        writer
            .add("A".to_string(), Duration::from_secs(30))
            .await
            .unwrap();
        writer
            .add("A".to_string(), Duration::from_secs(5))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    })
    .await;

    assert_eq!(vec!["A".to_string()], events);
}

/// Test to length the delay with a second call, which should be ignored.
#[actix_rt::test]
#[serial]
async fn test_inbox_longer() {
    common::init();

    let events = test_with(|writer| async move {
        writer
            .add("A".to_string(), Duration::from_secs(5))
            .await
            .unwrap();
        writer
            .add("A".to_string(), Duration::from_secs(30))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    })
    .await;

    assert_eq!(vec!["A".to_string()], events);
}

/// Don't wait as long as necessary. This should return nothing.
#[actix_rt::test]
#[serial]
async fn test_inbox_nothing() {
    common::init();

    let events = test_with(|writer| async move {
        writer
            .add("A".to_string(), Duration::from_secs(30))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    })
    .await;

    assert_eq!(Vec::<String>::new(), events);
}
