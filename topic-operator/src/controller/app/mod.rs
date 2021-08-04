mod topic;
mod user;

use topic::*;
use user::*;

use crate::{
    controller::{ControllerConfig, CONDITION_KAFKA_READY},
    data::*,
};
use async_trait::async_trait;
use drogue_client::{core::v1::Conditions, meta::v1::CommonMetadataMut, registry, Translator};
use drogue_cloud_operator_common::controller::{
    base::{
        ConditionExt, ControllerOperation, ProcessOutcome, ReadyState, StatusSection,
        CONDITION_RECONCILED,
    },
    reconciler::{
        progress::{
            self, application::ApplicationAccessor, OperationOutcome, Progressor, RunConstructor,
        },
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use drogue_cloud_service_common::kafka::{make_kafka_resource_name, ResourceType};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{ApiResource, DynamicObject},
    Api,
};
use operator_framework::install::Delete;
use std::{ops::Deref, time::Duration};

const FINALIZER: &str = "kafka";
const LABEL_KAFKA_CLUSTER: &str = "strimzi.io/cluster";
pub const ANNOTATION_APP_NAME: &str = "drogue.io/application-name";

pub struct ApplicationController {
    config: ControllerConfig,
    registry: registry::v1::Client,

    kafka_topic_resource: ApiResource,
    kafka_topics: Api<DynamicObject>,
    kafka_user_resource: ApiResource,
    kafka_users: Api<DynamicObject>,
    secrets: Api<Secret>,
}

impl ApplicationController {
    pub fn new(
        config: ControllerConfig,
        registry: registry::v1::Client,
        kafka_topic_resource: ApiResource,
        kafka_topics: Api<DynamicObject>,
        kafka_user_resource: ApiResource,
        kafka_users: Api<DynamicObject>,
        secrets: Api<Secret>,
    ) -> Self {
        Self {
            config,
            registry,
            kafka_topic_resource,
            kafka_topics,
            kafka_user_resource,
            kafka_users,
            secrets,
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
            kafka_topic_resource: &self.kafka_topic_resource,
            kafka_topics: &self.kafka_topics,
            kafka_user_resource: &self.kafka_user_resource,
            kafka_users: &self.kafka_users,
            secrets: &self.secrets,
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
    pub events_topic: Option<DynamicObject>,
    pub events_topic_name: Option<String>,
    pub app_user: Option<DynamicObject>,
    pub app_user_name: Option<String>,
}

pub struct DeconstructContext {
    pub app: registry::v1::Application,
    pub status: Option<KafkaAppStatus>,
}

pub struct ApplicationReconciler<'a> {
    pub config: &'a ControllerConfig,
    pub registry: &'a registry::v1::Client,
    pub kafka_topic_resource: &'a ApiResource,
    pub kafka_topics: &'a Api<DynamicObject>,
    pub kafka_user_resource: &'a ApiResource,
    pub kafka_users: &'a Api<DynamicObject>,
    pub secrets: &'a Api<Secret>,
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
        let status = app.section::<KafkaAppStatus>().and_then(|s| s.ok());

        let configured = app.metadata.finalizers.iter().any(|f| f == FINALIZER);
        let deleted = app.metadata.deletion_timestamp.is_some();

        Ok(match (configured, deleted) {
            (_, false) => ReconcileState::Construct(ConstructContext {
                app,
                events_topic: None,
                events_topic_name: None,
                app_user: None,
                app_user_name: None,
            }),
            (true, true) => ReconcileState::Deconstruct(DeconstructContext { app, status }),
            (false, true) => ReconcileState::Ignore(app),
        })
    }

    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        Progressor::<Self::Construct>::new(vec![
            Box::new(("HasFinalizer", |mut ctx: Self::Construct| async {
                // ensure we have a finalizer
                if ctx.app.metadata.ensure_finalizer(FINALIZER) {
                    // early return
                    Ok(OperationOutcome::Retry(ctx, None))
                } else {
                    Ok(OperationOutcome::Continue(ctx))
                }
            })),
            Box::new(CreateTopic {
                api: &self.kafka_topics,
                resource: &self.kafka_topic_resource,
                config: &self.config,
            }),
            Box::new(TopicReady {
                config: &self.config,
            }),
            Box::new(CreateUser {
                api: &self.kafka_users,
                resource: &self.kafka_user_resource,
                config: &self.config,
            }),
            Box::new(UserReady {
                config: &self.config,
                secrets: &self.secrets,
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

        let topic_name =
            make_kafka_resource_name(ResourceType::Events(ctx.app.metadata.name.clone()));

        let user_name =
            make_kafka_resource_name(ResourceType::Users(ctx.app.metadata.name.clone()));

        // remove topic

        self.kafka_topics
            .delete_optionally(&topic_name, &Default::default())
            .await?;
        self.kafka_users
            .delete_optionally(&user_name, &Default::default())
            .await?;

        // TODO: wait for resources to be actually deleted, then remove the finalizer

        // remove finalizer

        ctx.app.metadata.finalizers.retain(|f| f != FINALIZER);

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

impl StatusSection for KafkaAppStatus {
    fn ready_name() -> &'static str {
        CONDITION_KAFKA_READY
    }

    fn update_status(&mut self, conditions: Conditions, observed_generation: u64) {
        self.conditions = conditions;
        self.observed_generation = observed_generation;
    }
}

fn retry<C>(ctx: C) -> progress::Result<C>
where
    C: Send + Sync,
{
    Ok(OperationOutcome::Retry(ctx, Some(Duration::from_secs(15))))
}

fn condition_ready(condition: &str, resource: &DynamicObject) -> Option<bool> {
    resource.data["status"]["conditions"]
        .as_array()
        .and_then(|conditions| {
            conditions
                .iter()
                .filter_map(|cond| cond.as_object())
                .filter_map(|cond| {
                    if cond["type"] == condition {
                        match cond["status"].as_str() {
                            Some("True") => Some(true),
                            Some("False") => Some(false),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .next()
        })
}
