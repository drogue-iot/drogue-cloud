mod thing;

use thing::*;

use crate::{controller::ControllerConfig, data::DittoDeviceStatus, ditto::Client as DittoClient};
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
        ByDevice, ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use std::ops::Deref;
use tracing::instrument;

const FINALIZER: &str = "ditto";

pub struct DeviceController<TP>
where
    TP: TokenProvider,
{
    config: ControllerConfig,
    registry: registry::v1::Client<TP>,
    ditto: DittoClient,
    devops_provider: Option<AccessTokenProvider>,
    admin_provider: OpenIdTokenProvider,
}

impl<TP> DeviceController<TP>
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

        let admin_provider = config.ditto_admin.clone().discover_from().await?;

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
impl<TP>
    ControllerOperation<
        (String, String),
        (registry::v1::Application, registry::v1::Device),
        registry::v1::Device,
    > for DeviceController<TP>
where
    TP: TokenProvider,
{
    #[instrument(skip(self), fields(application=%input.0.metadata.name, device=%input.1.metadata.name))]
    async fn process_resource(
        &self,
        input: (registry::v1::Application, registry::v1::Device),
    ) -> Result<ProcessOutcome<registry::v1::Device>, ReconcileError> {
        ReconcileProcessor(DeviceReconciler {
            config: &self.config,
            registry: &self.registry,
            ditto: &self.ditto,
            devops_provider: &self.devops_provider,
            admin_provider: &self.admin_provider,
        })
        .reconcile(input)
        .await
    }

    #[instrument(skip(self), fields(application=%input.0.metadata.name, device=%input.1.metadata.name, message=message))]
    async fn recover(
        &self,
        message: &str,
        input: (registry::v1::Application, registry::v1::Device),
    ) -> Result<registry::v1::Device, ()> {
        let (app, mut device) = input;

        let mut conditions = device
            .section::<DittoDeviceStatus>()
            .and_then(|s| s.ok().map(|s| s.conditions))
            .unwrap_or_default();

        conditions.update(CONDITION_RECONCILED, ReadyState::Failed(message.into()));

        device
            .finish_ready::<DittoDeviceStatus>(conditions, app.metadata.generation)
            .map_err(|_| ())?;

        Ok(device)
    }
}

impl<TP> Deref for DeviceController<TP>
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
    pub device: registry::v1::Device,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub device: registry::v1::Device,
    pub status: Option<DittoDeviceStatus>,
}

pub struct DeviceReconciler<'a, TP>
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
impl<'a, TP> Reconciler for DeviceReconciler<'a, TP>
where
    TP: TokenProvider,
{
    type Input = (registry::v1::Application, registry::v1::Device);
    type Output = registry::v1::Device;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    #[instrument(skip(self), fields(application=%input.0.metadata.name, device=%input.1.metadata.name),ret)]
    async fn eval_state(
        &self,
        input: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        let (app, device) = input;
        Self::eval_by_finalizer(
            device.metadata.has_label_flag("ditto"),
            ByDevice(app, device),
            FINALIZER,
            |ByDevice(app, device)| ConstructContext { app, device },
            |ByDevice(app, device)| {
                let status = device.section::<DittoDeviceStatus>().and_then(|s| s.ok());
                DeconstructContext {
                    app,
                    device,
                    status,
                }
            },
            |ByDevice(_, device)| device,
        )
    }

    #[instrument(skip(self, ctx), ret)]
    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(HasFinalizer(FINALIZER)),
            Box::new(CreateThing {
                config: self.config,
                ditto: self.ditto,
                provider: self.admin_provider,
            }),
        ])
        .run_with::<DittoDeviceStatus>(ctx)
        .await
    }

    #[instrument(skip(self, ctx), ret)]
    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        DeleteThing {
            config: self.config,
            ditto: self.ditto,
            provider: self.admin_provider,
        }
        .run(&ctx)
        .await?;

        // cleanup

        ctx.device.clear_section::<DittoDeviceStatus>();
        ctx.device
            .update_section(|c: Conditions| c.clear_ready(DittoDeviceStatus::ready_name()))?;

        // remove finalizer

        ctx.device.metadata.remove_finalizer(FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.device))
    }
}

impl ResourceAccessor for ConstructContext {
    type Resource = registry::v1::Device;

    fn resource(&self) -> &Self::Resource {
        &self.device
    }

    fn resource_mut(&mut self) -> &mut Self::Resource {
        &mut self.device
    }

    fn into(self) -> Self::Resource {
        self.device
    }

    fn conditions(&self) -> Conditions {
        self.device
            .section::<DittoDeviceStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}
