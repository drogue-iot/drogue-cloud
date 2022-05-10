use crate::controller::base::{BaseController, ControllerOperation, Key};
use async_trait::async_trait;
use kube::Resource;
use std::{boxed::Box, sync::Arc};
use tokio::sync::Mutex;
use tracing::instrument;

#[async_trait]
pub trait EventProcessor<E>: Send + Sync {
    async fn handle(&self, event: &E) -> Result<bool, ()>;
}

pub struct FnEventProcessor<E, K, RI, RO, O>
where
    E: Send + Sync,
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    base_controller: Arc<Mutex<BaseController<K, RI, RO, O>>>,
    f: Box<dyn Fn(&E) -> Option<K> + Send + Sync>,
}

impl<E, K, RI, RO, O> FnEventProcessor<E, K, RI, RO, O>
where
    E: Send + Sync,
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    pub fn new<F>(base_controller: Arc<Mutex<BaseController<K, RI, RO, O>>>, f: F) -> Self
    where
        F: Fn(&E) -> Option<K> + Send + Sync + 'static,
    {
        Self {
            base_controller,
            f: Box::new(f),
        }
    }
}

#[async_trait]
impl<E, K, RI, RO, O> EventProcessor<E> for FnEventProcessor<E, K, RI, RO, O>
where
    E: Send + Sync + 'static,
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    async fn handle(&self, event: &E) -> Result<bool, ()> {
        if let Some(key) = (self.f)(event) {
            self.base_controller.lock().await.process(key).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Clone, Debug)]
pub enum NameSource {
    Name,
    Annotation(String),
    Label(String),
}

pub struct ResourceProcessor<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    controller: Arc<Mutex<BaseController<K, RI, RO, O>>>,
    /// The source for the name of the resource to reconcile
    source: NameSource,
}

impl<RI, RO, O> ResourceProcessor<String, RI, RO, O>
where
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<String, RI, RO> + Send + Sync + 'static,
{
    pub fn new(
        controller: Arc<Mutex<BaseController<String, RI, RO, O>>>,
        source: NameSource,
    ) -> Self {
        Self { controller, source }
    }

    fn extract<R: Resource>(&self, resource: &R) -> Option<String> {
        match &self.source {
            NameSource::Name => resource.meta().name.clone(),
            NameSource::Annotation(annotation) => resource
                .meta()
                .annotations
                .as_ref()
                .and_then(|a| a.get(annotation).cloned()),
            NameSource::Label(annotation) => resource
                .meta()
                .labels
                .as_ref()
                .and_then(|a| a.get(annotation).cloned()),
        }
    }
}

#[async_trait]
impl<R, RI, RO, O> EventProcessor<R> for ResourceProcessor<String, RI, RO, O>
where
    R: Resource + Send + Sync,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<String, RI, RO> + Send + Sync + 'static,
{
    #[instrument(skip_all, fields(meta=?event.meta()))]
    async fn handle(&self, event: &R) -> Result<bool, ()> {
        let key = self.extract(event);
        log::debug!("Extracted key from event: {:?}", key);
        if let Some(key) = key {
            self.controller.lock().await.process(key).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
