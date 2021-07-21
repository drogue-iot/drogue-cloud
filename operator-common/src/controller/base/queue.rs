use crate::controller::base::Key;
use chrono::{DateTime, Utc};
use deadpool_postgres::tokio_postgres::types::Type;
use deadpool_postgres::{Pool, PoolError};
use drogue_cloud_database_common::Client;
use serde::Deserialize;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct WorkQueueConfig {
    pub pg: deadpool_postgres::Config,
    pub instance: String,
}

pub struct WorkQueueWriter {
    instance: String,
    r#type: String,
    pool: Pool,
}

pub struct WorkQueueReader<K>
where
    K: Key,
{
    _marker: PhantomData<K>,
    running: Arc<AtomicBool>,
}

impl<K> WorkQueueReader<K>
where
    K: Key,
{
    pub fn new<F, Fut>(pool: Pool, instance: String, r#type: String, f: F) -> Self
    where
        F: Fn(K) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Option<(K, Duration)>, ()>> + Send,
    {
        let running = Arc::new(AtomicBool::new(true));
        let inner = InnerReader::<K> {
            _marker: PhantomData,
            instance: instance.clone(),
            r#type: r#type.clone(),
            pool: pool.clone(),
            running: running.clone(),
        };
        let writer = WorkQueueWriter {
            instance,
            r#type,
            pool,
        };
        tokio::spawn(async move {
            while let Some(entry) = inner.next().await {
                match f(entry.key.clone()).await {
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
        self.running.store(false, Ordering::Relaxed);
    }
}

impl WorkQueueWriter {
    pub fn new(instance: String, r#type: String, pool: Pool) -> Self {
        Self {
            instance,
            r#type,
            pool,
        }
    }

    pub async fn add<K>(&self, key: K, after: Duration) -> Result<(), ()>
    where
        K: Key,
    {
        Ok(self.insert(key, after).await.map_err(|_| ())?)
    }

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

        let r = c
            .execute(
                r#"
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
        GEN = WORKQUEUE.GEN + 1
    WHERE
            WORKQUEUE.TS > EXCLUDED.TS
        OR
            WORKQUEUE.TS < now()
"#,
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
    pub gen: u64,
}

struct InnerReader<K> {
    _marker: PhantomData<K>,
    running: Arc<AtomicBool>,
    instance: String,
    r#type: String,
    pool: Pool,
}

impl<K> InnerReader<K>
where
    K: Key,
{
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

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        log::info!("Exiting 'next' loop ...");

        None
    }

    async fn fetch(&self) -> Result<Option<Entry<K>>, anyhow::Error> {
        let c = self.pool.get().await?;

        let query = r#"
SELECT
    KEY,
    TS,
    GEN
FROM
    WORKQUEUE
WHERE
    INSTANCE = $1 AND
    TYPE = $2 AND
    TS > now()
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
                let gen = row.try_get::<_, i64>("GEN")? as u64;

                match K::from_string(key.clone()) {
                    Ok(key) => {
                        return Ok(Some(Entry {
                            key,
                            timestamp,
                            gen,
                        }))
                    }
                    Err(_) => {
                        log::info!("Failed to read next entry");
                        if let Err(err) = self.do_ack(key, timestamp, gen).await {
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

    async fn ack(&self, entry: Entry<K>) {
        if let Err(err) = self
            .do_ack(entry.key.to_string(), entry.timestamp, entry.gen)
            .await
        {
            // FIXME: need circuit breaker
            log::info!("Failed to acknowledge work queue entry: {}", err);
        }
    }

    async fn do_ack(&self, key: String, ts: DateTime<Utc>, gen: u64) -> Result<(), anyhow::Error> {
        let c = self.pool.get().await?;

        let r = c
            .execute(
                r#"
DELETE FROM WORKQUEUE WHERE
    INSTANCE = $1 AND
    TYPE = $2 AND
    KEY = $3 AND
    TS <= $4 AND
    GEN = $5
"#,
                &[&self.instance, &self.r#type, &key, &ts, &(gen as i64)],
            )
            .await;

        log::debug!("Delete result: {:?}", r);

        // try result

        r?;

        // done

        Ok(())
    }
}
