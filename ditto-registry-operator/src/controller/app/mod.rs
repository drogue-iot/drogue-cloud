mod app;
mod policy;

use app::*;
use policy::*;

use crate::{controller::ControllerConfig, data::DittoAppStatus, ditto::Client as DittoClient};
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::{CommonMetadataExt, CommonMetadataMut},
    openid::{AccessTokenProvider, OpenIdTokenProvider, TokenProvider},
    registry, Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{
        ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, StatusSection,
        CONDITION_RECONCILED,
    },
    reconciler::{
        operation::HasFinalizer,
        progress::{Progressor, ResourceAccessor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use std::ops::Deref;
use tracing::instrument;

const FINALIZER: &str = "ditto";

pub struct ApplicationController<TP>
where
    TP: TokenProvider,
{
    config: ControllerConfig,
    registry: registry::v1::Client<TP>,
    ditto: DittoClient,
    devops_provider: Option<AccessTokenProvider>,
    admin_provider: OpenIdTokenProvider,
}

impl<TP> ApplicationController<TP>
where
    TP: TokenProvider,
{
    pub async fn new(
        mut config: ControllerConfig,
        registry: registry::v1::Client<TP>,
        client: reqwest::Client,
    ) -> Result<Self, anyhow::Error> {
        let ditto = config.ditto_devops.clone();
        config.kafka = config.kafka.translate();

        let devops_provider = ditto
            .username
            .zip(ditto.password)
            .map(|(user, token)| AccessTokenProvider { user, token });

        let admin_provider = config
            .ditto_admin
            .clone()
            .discover_from(client.clone())
            .await?;

        Ok(Self {
            config,
            registry,
            ditto: DittoClient::new(client, ditto.url),
            devops_provider,
            admin_provider,
        })
    }
}

#[async_trait]
impl<TP> ControllerOperation<String, registry::v1::Application, registry::v1::Application>
    for ApplicationController<TP>
where
    TP: TokenProvider,
{
    #[instrument(skip(self), fields(application=%application.metadata.name))]
    async fn process_resource(
        &self,
        application: registry::v1::Application,
    ) -> Result<ProcessOutcome<registry::v1::Application>, ReconcileError> {
        ReconcileProcessor(ApplicationReconciler {
            config: &self.config,
            registry: &self.registry,
            ditto: &self.ditto,
            devops_provider: &self.devops_provider,
            admin_provider: &self.admin_provider,
        })
        .reconcile(application)
        .await
    }

    #[instrument(skip(self), fields(application=%app.metadata.name, message=message))]
    async fn recover(
        &self,
        message: &str,
        mut app: registry::v1::Application,
    ) -> Result<registry::v1::Application, ()> {
        let mut conditions = app
            .section::<DittoAppStatus>()
            .and_then(|s| s.ok().map(|s| s.conditions))
            .unwrap_or_default();

        conditions.update(CONDITION_RECONCILED, ReadyState::Failed(message.into()));

        app.finish_ready::<DittoAppStatus>(conditions, app.metadata.generation)
            .map_err(|_| ())?;

        Ok(app)
    }
}

impl<TP> Deref for ApplicationController<TP>
where
    TP: TokenProvider,
{
    type Target = registry::v1::Client<TP>;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

pub struct ConstructContext {
    pub app: registry::v1::Application,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<DittoAppStatus>,
}

pub struct ApplicationReconciler<'a, TP>
where
    TP: TokenProvider,
{
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client<TP>,
    pub ditto: &'a DittoClient,
    pub devops_provider: &'a Option<AccessTokenProvider>,
    pub admin_provider: &'a OpenIdTokenProvider,
}

#[async_trait]
impl<'a, TP> Reconciler for ApplicationReconciler<'a, TP>
where
    TP: TokenProvider,
{
    type Input = registry::v1::Application;
    type Output = registry::v1::Application;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    #[instrument(skip(self), fields(application=%app.metadata.name), err)]
    async fn eval_state(
        &self,
        app: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        Self::eval_by_finalizer(
            app.metadata.has_label_flag("ditto"),
            app,
            FINALIZER,
            |app| ConstructContext { app },
            |app| {
                let status = app.section::<DittoAppStatus>().and_then(|s| s.ok());
                DeconstructContext { app, status }
            },
            |app| app,
        )
    }

    #[instrument(skip(self, ctx), err)]
    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(HasFinalizer(FINALIZER)),
            Box::new(CreateApplication {
                config: self.config,
                ditto: self.ditto,
                provider: self.devops_provider,
            }),
            Box::new(CreatePolicy {
                config: self.config,
                ditto: self.ditto,
                provider: self.admin_provider,
            }),
        ])
        .run_with::<DittoAppStatus>(ctx)
        .await
    }

    #[instrument(skip(self, ctx), ret)]
    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        DeleteApplication {
            config: self.config,
            ditto: self.ditto,
            provider: self.devops_provider,
        }
        .run(&ctx)
        .await?;

        DeletePolicy {
            config: self.config,
            ditto: self.ditto,
            provider: self.admin_provider,
        }
        .run(&ctx)
        .await?;

        // remove finalizer

        ctx.app.metadata.remove_finalizer(FINALIZER);
        ctx.app
            .update_section(|c: Conditions| c.clear_ready(DittoAppStatus::ready_name()))?;

        // remove status

        ctx.app.clear_section::<DittoAppStatus>();

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }
}

impl ResourceAccessor for ConstructContext {
    type Resource = registry::v1::Application;

    fn resource(&self) -> &Self::Resource {
        &self.app
    }

    fn resource_mut(&mut self) -> &mut Self::Resource {
        &mut self.app
    }

    fn into(self) -> Self::Resource {
        self.app
    }

    fn conditions(&self) -> Conditions {
        self.app
            .section::<DittoAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}
