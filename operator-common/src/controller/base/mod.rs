mod app;
mod conditions;
mod device;
mod event;
pub mod queue;

pub use app::*;
pub use conditions::*;
pub use device::*;
pub use event::*;

use crate::controller::{
    base::queue::{WorkQueueConfig, WorkQueueHandler, WorkQueueReader, WorkQueueWriter},
    reconciler::ReconcileError,
};
use anyhow::Context;
use async_trait::async_trait;
use deadpool::Runtime;
use drogue_client::error::ClientError;
use std::{
    fmt::Debug,
    fmt::Formatter,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_postgres::NoTls;
use tracing::instrument;

pub const CONDITION_RECONCILED: &str = "Reconciled";

pub trait Key: Clone + Debug + Send + Sync + 'static {
    fn to_string(&self) -> String;
    fn from_string(s: String) -> Result<Self, &'static str>;
}

impl Key for String {
    fn to_string(&self) -> String {
        ToString::to_string(self)
    }

    fn from_string(s: String) -> Result<Self, &'static str> {
        Ok(s)
    }
}

impl Key for (String, String) {
    fn to_string(&self) -> String {
        format!("{}/{}", self.0, self.1)
    }

    fn from_string(s: String) -> Result<Self, &'static str> {
        match s.split('/').collect::<Vec<_>>().as_slice() {
            [a, b] => Ok((a.to_string(), b.to_string())),
            _ => Err("missing slash?"),
        }
    }
}

pub struct BaseController<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    writer: WorkQueueWriter,
    _reader: WorkQueueReader<K>,
    inner: Arc<Mutex<InnerBaseController<K, RI, RO, O>>>,
}

impl<K, RI, RO, O> BaseController<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    pub fn new<S: Into<String>>(
        config: WorkQueueConfig,
        r#type: S,
        operation: O,
    ) -> Result<Self, anyhow::Error> {
        let r#type = r#type.into();

        let inner = Arc::new(Mutex::new(InnerBaseController {
            _marker: PhantomData,
            operation,
        }));

        let pool = config
            .pg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create database pool")?;

        let instance = config.instance;

        let writer = WorkQueueWriter::new(pool.clone(), instance.clone(), r#type.clone());
        let reader = WorkQueueReader::new(pool, instance, r#type, Handler(inner.clone()));

        Ok(Self {
            writer,
            _reader: reader,
            inner,
        })
    }

    pub async fn process(&mut self, key: K) -> Result<(), ()> {
        if let Some(queue) = self.inner.lock().await.process(key).await? {
            self.writer.add(queue.0, queue.1).await?;
        }
        Ok(())
    }
}

struct Handler<K, RI, RO, O>(pub Arc<Mutex<InnerBaseController<K, RI, RO, O>>>)
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static;

#[async_trait]
impl<K, RI, RO, O> WorkQueueHandler<K> for Handler<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    async fn handle(&self, key: K) -> Result<Option<(K, Duration)>, ()> {
        self.0.lock().await.process(key).await
    }
}

struct InnerBaseController<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    _marker: PhantomData<(K, RI, RO)>,
    operation: O,
}

impl<K, RI, RO, O> InnerBaseController<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    const MAX_RETRIES: usize = 10;

    /// Process a key, locally retrying.
    ///
    /// This runs the operation, and does local retries if they are immediate.
    ///
    /// After a few retries, or when a long-term retry comes back, we forward that to the
    /// work queue and continue.
    pub async fn process(&mut self, key: K) -> Result<Option<(K, Duration)>, ()> {
        let mut retries: usize = 0;
        loop {
            let result = self.operation.process(&key).await;
            log::debug!("Processing({:?}/{}) -> {:?}", key, retries, result);
            match result {
                Ok(OperationOutcome::Complete) | Err(ReconcileError::Permanent(_)) => {
                    break Ok(None)
                }
                Ok(OperationOutcome::RetryNow) | Err(ReconcileError::Temporary(_)) => {
                    retries += 1;
                    if retries > Self::MAX_RETRIES {
                        log::debug!("Max retries reached, reschedule ...");
                        break Ok(Some((key, Duration::ZERO)));
                    } else {
                        log::debug!("Retry ...");
                        continue;
                    }
                }
                Ok(OperationOutcome::RetryLater(delay)) => {
                    break Ok(Some((key, delay)));
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum OperationOutcome {
    Complete,
    RetryNow,
    RetryLater(Duration),
}

impl OperationOutcome {
    pub fn retry(delay: Option<Duration>) -> Self {
        match delay {
            Some(delay) if !delay.is_zero() => Self::RetryLater(delay),
            _ => Self::RetryNow,
        }
    }
}

/// The resource operations are used to load and store the resource by its key.
#[async_trait]
pub trait ResourceOperations<K, RI, RO>
where
    K: Send + Sync,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
{
    /// Get the resource from the store.
    ///
    /// Returning [`None`] here will not trigger, but skip the operation.
    async fn get(&self, key: &K) -> Result<Option<RI>, ClientError>;

    /// Update the resource in the store, if it did change.
    async fn update_if(&self, original: &RO, current: RO) -> Result<(), ReconcileError>;

    fn ref_output(input: &RI) -> &RO;
}

#[async_trait]
pub trait ControllerOperation<K, RI, RO>: ResourceOperations<K, RI, RO>
where
    K: Debug + Send + Sync,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
{
    async fn process_resource(&self, resource: RI) -> Result<ProcessOutcome<RO>, ReconcileError>;

    #[instrument(skip(self), ret)]
    /// Process the key, any permanent error returned is a fatal error,
    async fn process(&self, key: &K) -> Result<OperationOutcome, ReconcileError> {
        // read the resource ...
        match self.get(key).await {
            // ... and process it
            Ok(Some(resource)) => match self.process_resource(resource.clone()).await {
                // ... completed -> store and return(done)
                Ok(ProcessOutcome::Complete(outcome)) => {
                    self.update_if(Self::ref_output(&resource), outcome).await?;
                    Ok(OperationOutcome::Complete)
                }
                // ... need to re-try -> store and return(retry)
                Ok(ProcessOutcome::Retry(outcome, delay)) => {
                    self.update_if(Self::ref_output(&resource), outcome).await?;
                    Ok(OperationOutcome::retry(delay))
                }
                Err(ReconcileError::Temporary(msg)) => {
                    let outcome = self
                        .recover(&msg, resource.clone())
                        .await
                        .map_err(|_| ReconcileError::permanent("Failed to recover"))?;
                    self.update_if(Self::ref_output(&resource), outcome).await?;
                    Ok(OperationOutcome::RetryNow)
                }
                Err(ReconcileError::Permanent(msg)) => {
                    let outcome = self
                        .recover(&msg, resource.clone())
                        .await
                        .map_err(|_| ReconcileError::permanent("Failed to recover"))?;
                    self.update_if(Self::ref_output(&resource), outcome).await?;
                    Ok(OperationOutcome::Complete)
                }
            },
            // ... nothing found -> we are done here
            Ok(None) => {
                // resource is gone, we have finalizers to guard against this
                Ok(OperationOutcome::Complete)
            }
            // ... error -> retry
            Err(err) => {
                log::debug!("Reconciliation failed (RetryNow): {}", err);
                Ok(OperationOutcome::RetryNow)
            }
        }
    }

    /// Recover from a reconciliation error.
    ///
    /// The returned resource will be stored. Returning an error here, means a fatal error
    /// which will be reported back to the event source.
    async fn recover(&self, message: &str, resource: RI) -> Result<RO, ()>;
}

#[derive(Clone)]
pub enum ProcessOutcome<T> {
    Complete(T),
    Retry(T, Option<Duration>),
}

impl<T> Debug for ProcessOutcome<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry(_, delay) => f.debug_tuple("Retry").field(&"...").field(delay).finish(),
            Self::Complete(_) => f.debug_tuple("Complete").field(&"...").finish(),
        }
    }
}

impl<T> Deref for ProcessOutcome<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Complete(result) => result,
            Self::Retry(result, _) => result,
        }
    }
}

impl<T> DerefMut for ProcessOutcome<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Complete(ref mut result) => result,
            Self::Retry(ref mut result, _) => result,
        }
    }
}
