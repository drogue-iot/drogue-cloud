mod thing;

use thing::*;

use crate::{controller::ControllerConfig, data::DittoDeviceStatus, ditto::Client as DittoClient};
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::CommonMetadataMut,
    openid::{AccessTokenProvider, OpenIdTokenProvider, TokenProvider},
    registry, Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, CONDITION_RECONCILED},
    reconciler::{
        operation::{HasFinalizer, MetadataContext},
        progress::{Progressor, ResourceAccessor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use std::ops::Deref;

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
impl<TP>
    ControllerOperation<
        (String, String),
        (registry::v1::Application, registry::v1::Device),
        registry::v1::Device,
    > for DeviceController<TP>
where
    TP: TokenProvider,
{
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

    async fn recover(
        &self,
        message: &str,
        (app, mut device): (registry::v1::Application, registry::v1::Device),
    ) -> Result<registry::v1::Device, ()> {
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

impl MetadataContext for ConstructContext {
    fn as_metadata_mut(&mut self) -> &mut dyn CommonMetadataMut {
        &mut self.device.metadata
    }
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

    async fn eval_state(
        &self,
        (app, device): Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        let status = device.section::<DittoDeviceStatus>().and_then(|s| s.ok());

        let configured = device.metadata.finalizers.iter().any(|f| f == FINALIZER);
        let deleted = device.metadata.deletion_timestamp.is_some();

        Ok(match (configured, deleted) {
            (_, false) => ReconcileState::Construct(ConstructContext { app, device }),
            (true, true) => ReconcileState::Deconstruct(DeconstructContext {
                app,
                device,
                status,
            }),
            (false, true) => ReconcileState::Ignore(device),
        })
    }

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
