use crate::controller::base::Key;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::{tokio_postgres::types::Type, Pool, PoolError};
use drogue_cloud_database_common::{postgres, Client};
use serde::Deserialize;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::instrument;

#[derive(Clone, Debug, Deserialize)]
pub struct WorkQueueConfig {
    pub pg: postgres::Config,
    pub instance: String,
}

pub struct WorkQueueWriter {
    instance: String,
    r#type: String,
    pool: Pool,
}

impl Debug for WorkQueueWriter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkQueueWriter")
            .field("instance", &self.instance)
            .field("type", &self.r#type)
            .field("pool", &"...")
            .finish()
    }
}

pub struct WorkQueueReader<K>
where
    K: Key,
{
    _marker: PhantomData<K>,
    running: Arc<AtomicBool>,
}

#[async_trait]
pub trait WorkQueueHandler<K>: Send + Sync {
    async fn handle(&self, key: K) -> Result<Option<(K, Duration)>, ()>;
}

#[async_trait]
impl<K, F, Fut> WorkQueueHandler<K> for F
where
    K: Key,
    F: Fn(K) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<(K, Duration)>, ()>> + Send,
{
    async fn handle(&self, key: K) -> Result<Option<(K, Duration)>, ()> {
        (self)(key).await
    }
}

#[derive(Clone, Debug)]
pub struct WorkQueueReaderOptions {
    pub delay: Duration,
}

impl<K> WorkQueueReader<K>
where
    K: Key,
{
    pub fn new<H>(pool: Pool, instance: String, r#type: String, handler: H) -> Self
    where
        H: WorkQueueHandler<K> + 'static,
    {
        Self::with_options(
            pool,
            instance,
            r#type,
            handler,
            WorkQueueReaderOptions {
                delay: Duration::from_secs(5),
            },
        )
    }

    pub fn with_options<H>(
        pool: Pool,
        instance: String,
        r#type: String,
        handler: H,
        opts: WorkQueueReaderOptions,
    ) -> Self
    where
        H: WorkQueueHandler<K> + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let inner = InnerReader::<K> {
            _marker: PhantomData,
            instance: instance.clone(),
            r#type: r#type.clone(),
            pool: pool.clone(),
            running: running.clone(),
            delay: opts.delay,
        };
        let writer = WorkQueueWriter {
            instance,
            r#type,
            pool,
        };
        tokio::spawn(async move {
            while let Some(entry) = inner.next().await {
                match handler.handle(entry.key.clone()).await {
                    Ok(Some((rt, after))) => {
                        if writer.add(rt, after).await.is_ok() {
                            inner.ack(entry).await;
                        }
                    }
                    Ok(None) => {
                        inner.ack(entry).await;
                    }
                    _ => {}
                }
            }
            log::info!("Exiting worker loop");
        });
        Self {
            _marker: PhantomData,
            running,
        }
    }
}

impl<K> Drop for WorkQueueReader<K>
where
    K: Key,
{
    fn drop(&mut self) {
        log::info!("Dropping work queue reader");
        self.running.store(false, Ordering::Relaxed);
    }
}

impl WorkQueueWriter {
    pub fn new(pool: Pool, instance: String, r#type: String) -> Self {
        Self {
            instance,
            r#type,
            pool,
        }
    }

    #[instrument(err(Debug))]
    pub async fn add<K>(&self, key: K, after: Duration) -> Result<(), ()>
    where
        K: Key,
    {
        Ok(self.insert(key, after).await.map_err(|_| ())?)
    }

    #[instrument(err)]
    async fn insert<K>(&self, key: K, after: Duration) -> Result<(), PoolError>
    where
        K: Key,
    {
        let c = self.pool.get().await?;

        let after =
            chrono::Duration::from_std(after).unwrap_or_else(|_| chrono::Duration::max_value());
        let ts = Utc::now() + after;

        // We resolve conflicts in the database by either shortening the delay or by moving
        // if in the future, if the current date is in the past.
        //
        // If the date is in the past, it is expected to be processed already. If that didn't
        // happen, we still get a chance for doing so in the future. If the entry is currently
        // being processed, we (always) increment the generation. So that the current iteration will
        // not delete the entry, as that will only check the current iteration generation.

        let sql = r#"
INSERT INTO WORKQUEUE (
    INSTANCE,
    TYPE,
    KEY,
    TS
) VALUES (
    $1,
    $2,
    $3,
    $4
)
ON CONFLICT (INSTANCE, TYPE, KEY) 
DO
    UPDATE SET
        TS = EXCLUDED.TS,
        REV = WORKQUEUE.REV + 1
    WHERE
            WORKQUEUE.TS > EXCLUDED.TS
        OR
            WORKQUEUE.TS < now()
"#;

        let stmt = c
            .prepare_typed(
                sql,
                &[
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::TIMESTAMPTZ,
                ],
            )
            .await?;

        let r = c
            .execute(
                &stmt,
                &[&self.instance, &self.r#type, &key.to_string(), &ts],
            )
            .await;

        log::debug!("Insert result: {:?}", r);

        // try result

        r?;

        // done

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Entry<K: Key> {
    pub key: K,
    pub timestamp: DateTime<Utc>,
    pub rev: u64,
}

struct InnerReader<K> {
    _marker: PhantomData<K>,
    running: Arc<AtomicBool>,
    instance: String,
    r#type: String,
    pool: Pool,
    delay: Duration,
}

impl<K> InnerReader<K>
where
    K: Key,
{
    #[instrument(skip(self), level = "debug", ret)]
    async fn next(&self) -> Option<Entry<K>> {
        while self.running.load(Ordering::Relaxed) {
            match self.fetch().await {
                Ok(Some(next)) => {
                    log::debug!("Next key: {:?}", next);
                    return Some(next);
                }
                Err(err) => {
                    log::info!("Failed to fetch next entry: {}", err);
                }
                _ => {}
            }

            tokio::time::sleep(self.delay).await;
        }

        log::info!("Exiting 'next' loop ...");

        None
    }

    #[instrument(skip(self), level = "debug", ret, err)]
    async fn fetch(&self) -> Result<Option<Entry<K>>, anyhow::Error> {
        let c = self.pool.get().await?;

        let query = r#"
SELECT
    KEY,
    TS,
    REV
FROM
    WORKQUEUE
WHERE
    INSTANCE = $1 AND
    TYPE = $2 AND
    TS < now()
ORDER BY
    TS ASC
LIMIT 1
"#;

        let stmt = c
            .prepare_typed(query, &[Type::VARCHAR, Type::VARCHAR])
            .await?;

        loop {
            if let Some(row) = c.query_opt(&stmt, &[&self.instance, &self.r#type]).await? {
                let key: String = row.try_get("KEY")?;
                let timestamp = row.try_get("TS")?;
                let rev = row.try_get::<_, i64>("REV")? as u64;

                match K::from_string(key.clone()) {
                    Ok(key) => {
                        return Ok(Some(Entry {
                            key,
                            timestamp,
                            rev,
                        }))
                    }
                    Err(_) => {
                        log::info!("Failed to read next entry");
                        if let Err(err) = self.do_ack(key, timestamp, rev).await {
                            // FIXME: circuit breaker
                            log::warn!("Failed to ack invalid entry: {}", err);
                        }
                    }
                };
            } else {
                return Ok(None);
            }
        }
    }

    #[instrument(skip(self))]
    async fn ack(&self, entry: Entry<K>) {
        if let Err(err) = self
            .do_ack(entry.key.to_string(), entry.timestamp, entry.rev)
            .await
        {
            // FIXME: need circuit breaker
            log::info!("Failed to acknowledge work queue entry: {}", err);
        }
    }

    #[instrument(skip(self), ret, err)]
    async fn do_ack(&self, key: String, ts: DateTime<Utc>, rev: u64) -> Result<(), anyhow::Error> {
        let c = self.pool.get().await?;

        let sql = r#"
DELETE FROM WORKQUEUE WHERE
    INSTANCE = $1 AND
    TYPE = $2 AND
    KEY = $3 AND
    TS <= $4 AND
    REV = $5
"#;
        let stmt = c
            .prepare_typed(
                sql,
                &[
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::TIMESTAMPTZ,
                    Type::INT8,
                ],
            )
            .await?;

        let r = c
            .execute(
                &stmt,
                &[&self.instance, &self.r#type, &key, &ts, &(rev as i64)],
            )
            .await;

        log::debug!("Delete result: {:?}", r);

        // try result

        r?;

        // done

        Ok(())
    }
}
