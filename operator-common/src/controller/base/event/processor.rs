use crate::controller::base::{BaseController, ControllerOperation, Key};
use async_std::sync::Mutex;
use async_trait::async_trait;
use kube::api::DynamicObject;
use std::{boxed::Box, sync::Arc};

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

pub struct ResourceProcessor<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    controller: Arc<Mutex<BaseController<K, RI, RO, O>>>,
    annotation: String,
}

impl<RI, RO, O> ResourceProcessor<String, RI, RO, O>
where
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<String, RI, RO> + Send + Sync + 'static,
{
    pub fn new<S>(controller: Arc<Mutex<BaseController<String, RI, RO, O>>>, annotation: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            controller,
            annotation: annotation.into(),
        }
    }

    fn extract(&self, resource: &DynamicObject) -> Option<String> {
        resource.metadata.annotations.get(&self.annotation).cloned()
    }
}

#[async_trait]
impl<RI, RO, O> EventProcessor<DynamicObject> for ResourceProcessor<String, RI, RO, O>
where
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<String, RI, RO> + Send + Sync + 'static,
{
    async fn handle(&self, event: &DynamicObject) -> Result<bool, ()> {
        if let Some(key) = self.extract(event) {
            self.controller.lock().await.process(key).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
