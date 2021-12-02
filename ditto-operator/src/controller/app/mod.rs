mod ditto;

use ditto::*;

use crate::{controller::ControllerConfig, ditto::Client as DittoClient};
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::CommonMetadataMut,
    openid::TokenProvider,
    registry::{self, v1::KafkaAppStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, CONDITION_RECONCILED},
    reconciler::{
        operation::{HasFinalizer, MetadataContext},
        progress::{application::ApplicationAccessor, Progressor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use std::ops::Deref;

const FINALIZER: &str = "ditto";

pub struct ApplicationController<TP: TokenProvider> {
    config: ControllerConfig,
    registry: registry::v1::Client<TP>,
    ditto: DittoClient,
}

impl<TP: TokenProvider> ApplicationController<TP> {
    pub fn new(config: ControllerConfig, registry: registry::v1::Client<TP>) -> Self {
        let ditto = config.ditto_devops.clone();
        Self {
            config,
            registry,
            ditto: DittoClient::new(
                reqwest::Client::default(),
                ditto.url,
                ditto.username,
                ditto.password,
            ),
        }
    }
}

#[async_trait]
impl<TP: TokenProvider>
    ControllerOperation<String, registry::v1::Application, registry::v1::Application>
    for ApplicationController<TP>
{
    async fn process_resource(
        &self,
        application: registry::v1::Application,
    ) -> Result<ProcessOutcome<registry::v1::Application>, ReconcileError> {
        ReconcileProcessor(ApplicationReconciler {
            config: &self.config,
            registry: &self.registry,
            ditto: &self.ditto,
        })
        .reconcile(application)
        .await
    }

    async fn recover(
        &self,
        message: &str,
        mut app: registry::v1::Application,
    ) -> Result<registry::v1::Application, ()> {
        let mut conditions = app
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok().map(|s| s.conditions))
            .unwrap_or_default();

        conditions.update(CONDITION_RECONCILED, ReadyState::Failed(message.into()));

        app.finish_ready::<KafkaAppStatus>(conditions, app.metadata.generation)
            .map_err(|_| ())?;

        Ok(app)
    }
}

impl<TP: TokenProvider> Deref for ApplicationController<TP> {
    type Target = registry::v1::Client<TP>;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

pub struct ConstructContext {
    pub app: registry::v1::Application,
}

impl MetadataContext for ConstructContext {
    fn as_metadata_mut(&mut self) -> &mut dyn CommonMetadataMut {
        &mut self.app.metadata
    }
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<KafkaAppStatus>,
}

pub struct ApplicationReconciler<'a, TP: TokenProvider> {
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client<TP>,
    pub ditto: &'a DittoClient,
}

#[async_trait]
impl<'a, TP: TokenProvider> Reconciler for ApplicationReconciler<'a, TP> {
    type Input = registry::v1::Application;
    type Output = registry::v1::Application;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    async fn eval_state(
        &self,
        app: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        let status = app.section::<KafkaAppStatus>().and_then(|s| s.ok());

        let configured = app.metadata.finalizers.iter().any(|f| f == FINALIZER);
        let deleted = app.metadata.deletion_timestamp.is_some();

        Ok(match (configured, deleted) {
            (_, false) => ReconcileState::Construct(ConstructContext { app }),
            (true, true) => ReconcileState::Deconstruct(DeconstructContext { app, status }),
            (false, true) => ReconcileState::Ignore(app),
        })
    }

    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(HasFinalizer(FINALIZER)),
            Box::new(CreateApplication {
                config: self.config,
                ditto: self.ditto,
            }),
        ])
        .run_with::<KafkaAppStatus>(ctx)
        .await
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        DeleteApplication {
            config: self.config,
            ditto: self.ditto,
        }
        .run(&mut ctx)
        .await?;

        // remove finalizer

        ctx.app.metadata.remove_finalizer(FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }
}

impl ApplicationAccessor for ConstructContext {
    fn app(&self) -> &registry::v1::Application {
        &self.app
    }

    fn app_mut(&mut self) -> &mut registry::v1::Application {
        &mut self.app
    }

    fn into(self) -> registry::v1::Application {
        self.app
    }

    fn conditions(&self) -> Conditions {
        self.app
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}
