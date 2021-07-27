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
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use drogue_client::error::ClientError;
use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio_postgres::NoTls;

pub const CONDITION_RECONCILED: &str = "Reconciled";

pub trait Key: Clone + Debug + Send + Sync + 'static {
    fn to_string(&self) -> String;
    fn from_string(s: String) -> Result<Self, ()>;
}

impl Key for String {
    fn to_string(&self) -> String {
        ToString::to_string(self)
    }

    fn from_string(s: String) -> Result<Self, ()> {
        Ok(s)
    }
}

impl Key for (String, String) {
    fn to_string(&self) -> String {
        format!("{}/{}", self.0, self.1)
    }

    fn from_string(s: String) -> Result<Self, ()> {
        match s.split('/').collect::<Vec<_>>().as_slice() {
            [a, b] => Ok((a.to_string(), b.to_string())),
            _ => Err(()),
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
            .create_pool(NoTls)
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
            match self.operation.process(&key).await? {
                OperationOutcome::Complete => break Ok(None),
                OperationOutcome::RetryNow => {
                    retries += 1;
                    if retries > Self::MAX_RETRIES {
                        break Ok(Some((key, Duration::ZERO)));
                    } else {
                        continue;
                    }
                }
                OperationOutcome::RetryLater(delay) => {
                    break Ok(Some((key, delay)));
                }
            }
        }
    }
}

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

#[async_trait]
pub trait ResourceOperations<K, RI, RO>
where
    K: Send + Sync,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
{
    /// Get the resource.
    async fn get(&self, key: &K) -> Result<Option<RI>, ClientError<reqwest::Error>>;

    /// Update the resource if it did change.
    async fn update_if(&self, original: &RO, current: &RO) -> Result<OperationOutcome, ()>;

    fn ref_output(input: &RI) -> &RO;
}

#[async_trait]
pub trait ControllerOperation<K, RI, RO>: ResourceOperations<K, RI, RO>
where
    K: Send + Sync,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
{
    async fn process_resource(&self, application: RI)
        -> Result<ProcessOutcome<RO>, ReconcileError>;

    /// Process the key, any error returned is a fatal error,
    async fn process(&self, key: &K) -> Result<OperationOutcome, ()> {
        // read the resource ...
        match self.get(key).await {
            // ... and process it
            Ok(Some(resource)) => match self.process_resource(resource.clone()).await {
                // ... completed -> store and return(done)
                Ok(ProcessOutcome::Complete(outcome)) => {
                    self.update_if(Self::ref_output(&resource), &outcome)
                        .await?;
                    Ok(OperationOutcome::Complete)
                }
                // ... need to re-try -> store and return(retry)
                Ok(ProcessOutcome::Retry(outcome, delay)) => {
                    self.update_if(Self::ref_output(&resource), &outcome)
                        .await?;
                    Ok(OperationOutcome::retry(delay))
                }
                Err(ReconcileError::Temporary(msg)) => {
                    let outcome = self.recover(&msg, resource.clone()).await?;
                    self.update_if(Self::ref_output(&resource), &outcome)
                        .await?;
                    Ok(OperationOutcome::RetryNow)
                }
                Err(ReconcileError::Permanent(msg)) => {
                    let outcome = self.recover(&msg, resource.clone()).await?;
                    self.update_if(Self::ref_output(&resource), &outcome)
                        .await?;
                    Ok(OperationOutcome::Complete)
                }
            },
            // ... nothing found -> we are done here
            Ok(None) => {
                // resource is gone, we have finalizers to guard against this
                Ok(OperationOutcome::Complete)
            }
            // ... error -> retry
            Err(_) => Ok(OperationOutcome::RetryNow),
        }
    }

    /// Recover from a reconciliation error.
    ///
    /// The returned resource will be stored. Returning an error here, means a fatal error
    /// which will be reported back to the event source.
    async fn recover(&self, message: &str, resource: RI) -> Result<RO, ()>;
}

#[derive(Clone, Debug)]
pub enum ProcessOutcome<T> {
    Complete(T),
    Retry(T, Option<Duration>),
}

impl<T> Deref for ProcessOutcome<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Complete(result) => &result,
            Self::Retry(result, _) => &result,
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
