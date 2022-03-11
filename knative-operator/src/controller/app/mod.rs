mod source;

use source::*;

use crate::controller::ControllerConfig;
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::CommonMetadataMut,
    openid::TokenProvider,
    registry::{
        self,
        v1::{KnativeAppSpec, KnativeAppStatus},
    },
    Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{
        ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, StatusSection,
        CONDITION_RECONCILED,
    },
    reconciler::{
        operation::HasFinalizer,
        progress::{self, OperationOutcome, Progressor, ResourceAccessor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use k8s_openapi::api::apps::v1::Deployment;
use kube::Api;
use operator_framework::install::Delete;
use std::{ops::Deref, time::Duration};

const FINALIZER: &str = "knative";

pub struct ApplicationController<TP: TokenProvider> {
    config: ControllerConfig,
    registry: registry::v1::Client<TP>,

    deployments: Api<Deployment>,
}

impl<TP: TokenProvider> ApplicationController<TP> {
    pub fn new(
        config: ControllerConfig,
        registry: registry::v1::Client<TP>,
        deployments: Api<Deployment>,
    ) -> Self {
        Self {
            config,
            registry,
            deployments,
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
            deployments: &self.deployments,
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
            .section::<KnativeAppStatus>()
            .and_then(|s| s.ok().map(|s| s.conditions))
            .unwrap_or_default();

        conditions.update(CONDITION_RECONCILED, ReadyState::Failed(message.into()));

        app.finish_ready::<KnativeAppStatus>(conditions, app.metadata.generation)
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
    pub deployment: Option<Deployment>,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<KnativeAppStatus>,
}

pub struct ApplicationReconciler<'a, TP: TokenProvider> {
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client<TP>,
    pub deployments: &'a Api<Deployment>,
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
        let spec = app.section::<KnativeAppSpec>();
        let requested = match &spec {
            Some(Ok(spec)) => !spec.disabled,
            Some(_) => true,
            None => false,
        };

        Self::eval_by_finalizer(
            requested,
            app,
            FINALIZER,
            |app| ConstructContext {
                app,
                deployment: None,
            },
            |app| {
                let status = app.section::<KnativeAppStatus>().and_then(|s| s.ok());
                DeconstructContext { app, status }
            },
            |app| app,
        )
    }

    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(HasFinalizer(FINALIZER)),
            Box::new(CreateDeployment {
                deployments: self.deployments,
                config: self.config,
            }),
            Box::new(SourceReady {
                config: self.config,
            }),
        ])
        .run_with::<KnativeAppStatus>(ctx)
        .await
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // delete

        let topic_name = make_kafka_resource_name(ResourceType::Events(&ctx.app.metadata.name));

        // remove deployment

        if self
            .deployments
            .delete_optionally(&topic_name, &Default::default())
            .await?
        {
            // remove finalizer

            ctx.app.metadata.remove_finalizer(FINALIZER);

            // remove status

            ctx.app
                .update_section(|c: Conditions| c.clear_ready(KnativeAppStatus::ready_name()))?;
            ctx.app.clear_section::<KnativeAppStatus>();

            // done

            Ok(ProcessOutcome::Complete(ctx.app))
        } else {
            // need to wait until it is gone

            Ok(ProcessOutcome::Retry(
                ctx.app,
                Some(Duration::from_secs(10)),
            ))
        }
    }
}

impl ResourceAccessor for ConstructContext {
    type Resource = registry::v1::Application;

    fn resource(&self) -> &registry::v1::Application {
        &self.app
    }

    fn resource_mut(&mut self) -> &mut registry::v1::Application {
        &mut self.app
    }

    fn into(self) -> registry::v1::Application {
        self.app
    }

    fn conditions(&self) -> Conditions {
        self.app
            .section::<KnativeAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}

fn retry<C>(ctx: C) -> progress::Result<C>
where
    C: Send + Sync,
{
    Ok(OperationOutcome::Retry(ctx, Some(Duration::from_secs(15))))
}

fn condition_ready(condition: &str, resource: &Deployment) -> Option<bool> {
    resource
        .status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .and_then(|conditions| {
            conditions
                .iter()
                .filter_map(|cond| {
                    if cond.type_ == condition {
                        match cond.status.as_str() {
                            "True" => Some(true),
                            "False" => Some(false),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .next()
        })
}
