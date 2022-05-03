mod topic;

use topic::*;

use crate::{controller::ControllerConfig, kafka::TopicErrorConverter};
use async_trait::async_trait;
use drogue_client::{
    core::v1::Conditions,
    meta::v1::CommonMetadataMut,
    registry::{self, v1::KafkaAppStatus},
    Translator,
};
use drogue_cloud_operator_common::controller::{
    base::{ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, CONDITION_RECONCILED},
    reconciler::{
        operation::HasFinalizer,
        progress::{Progressor, ResourceAccessor, RunConstructor},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use rdkafka::{
    admin::{AdminClient, AdminOptions},
    client::DefaultClientContext,
    error::{KafkaError, RDKafkaErrorCode},
};
use std::ops::Deref;

const FINALIZER: &str = "kafka-topic";

pub struct ApplicationController {
    config: ControllerConfig,
    registry: registry::v1::Client,
    admin: AdminClient<DefaultClientContext>,
}

impl ApplicationController {
    pub fn new(
        config: ControllerConfig,
        registry: registry::v1::Client,
        admin: AdminClient<DefaultClientContext>,
    ) -> Self {
        Self {
            config: config.translate(),
            registry,
            admin,
        }
    }
}

#[async_trait]
impl ControllerOperation<String, registry::v1::Application, registry::v1::Application>
    for ApplicationController
{
    async fn process_resource(
        &self,
        application: registry::v1::Application,
    ) -> Result<ProcessOutcome<registry::v1::Application>, ReconcileError> {
        ReconcileProcessor(ApplicationReconciler {
            config: &self.config,
            registry: &self.registry,
            admin: &self.admin,
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

impl Deref for ApplicationController {
    type Target = registry::v1::Client;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

pub struct ConstructContext {
    pub app: registry::v1::Application,
    pub events_topic_name: Option<String>,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<KafkaAppStatus>,
}

pub struct ApplicationReconciler<'a> {
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client,
    pub admin: &'a AdminClient<DefaultClientContext>,
}

#[async_trait]
impl<'a> Reconciler for ApplicationReconciler<'a> {
    type Input = registry::v1::Application;
    type Output = registry::v1::Application;
    type Construct = ConstructContext;
    type Deconstruct = DeconstructContext;

    async fn eval_state(
        &self,
        app: Self::Input,
    ) -> Result<ReconcileState<Self::Output, Self::Construct, Self::Deconstruct>, ReconcileError>
    {
        Self::eval_by_finalizer(
            true,
            app,
            FINALIZER,
            |app| ConstructContext {
                app,
                events_topic_name: None,
            },
            |app| {
                let status = app.section::<KafkaAppStatus>().and_then(|s| s.ok());
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
            Box::new(CreateTopic {
                config: self.config,
                admin: self.admin,
            }),
        ])
        .run_with::<KafkaAppStatus>(ctx)
        .await
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // delete

        let topic_name = make_kafka_resource_name(ResourceType::Events(&ctx.app.metadata.name));

        match self
            .admin
            .delete_topics(&[&topic_name], &AdminOptions::new())
            .await
            .single_topic_response()
        {
            Ok(_) => {
                log::info!("Topic {} deleted", topic_name);
            }
            Err(KafkaError::AdminOp(RDKafkaErrorCode::UnknownTopic)) => {
                log::info!("Topic {} was already deleted", topic_name);
            }
            Err(KafkaError::AdminOp(RDKafkaErrorCode::BrokerTransportFailure)) => {
                let err = KafkaError::AdminOp(RDKafkaErrorCode::BrokerTransportFailure);
                log::warn!("Failed to create topic ({}): {:?}", topic_name, err);
                return Err(ReconcileError::temporary(format!(
                    "Failed to create topic: {}",
                    err
                )));
            }
            Err(err) => {
                log::warn!("Failed to delete topic: {:?}", err);
                return Err(ReconcileError::permanent(format!(
                    "Failed to delete topic: {}",
                    err
                )));
            }
        }

        // remove finalizer

        ctx.app.metadata.remove_finalizer(FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
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
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions
    }
}
