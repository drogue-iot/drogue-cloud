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
        construct::{self, ConstructOperation, Construction, Constructor, Outcome},
        ReconcileError, ReconcileProcessor, ReconcileState, Reconciler,
    },
};
use drogue_cloud_service_api::events::EventTarget;
use drogue_cloud_service_common::kafka::make_topic_resource_name;
use kube::{
    api::{ApiResource, DynamicObject},
    Api, Resource,
};
use operator_framework::{install::Delete, process::create_or_update_by};
use serde_json::json;
use std::{ops::Deref, time::Duration};

const FINALIZER: &str = "kafka";
const LABEL_KAFKA_CLUSTER: &str = "strimzi.io/cluster";
const ANNOTATION_APP_NAME: &str = "drogue.io/application-name";

pub struct ApplicationController {
    config: ControllerConfig,
    registry: registry::v1::Client,

    kafka_topic_resource: ApiResource,
    kafka_topics: Api<DynamicObject>,
}

impl ApplicationController {
    pub fn new(
        config: ControllerConfig,
        registry: registry::v1::Client,
        kafka_topic_resource: ApiResource,
        kafka_topics: Api<DynamicObject>,
    ) -> Self {
        Self {
            config,
            registry,
            kafka_topic_resource,
            kafka_topics,
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
            }),
            (true, true) => ReconcileState::Deconstruct(DeconstructContext { app, status }),
            (false, true) => ReconcileState::Ignore(app),
        })
    }

    async fn construct(
        &self,
        ctx: Self::Construct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        let constructor = Constructor::<Self::Construct>::new(vec![
            Box::new(("HasFinalizer", |mut ctx: Self::Construct| async {
                // ensure we have a finalizer
                if ctx.app.metadata.ensure_finalizer(FINALIZER) {
                    // early return
                    Ok(Outcome::Retry(ctx, None))
                } else {
                    Ok(Outcome::Continue(ctx))
                }
            })),
            Box::new(CreateTopic {
                api: &self.kafka_topics,
                resource: &self.kafka_topic_resource,
                config: &self.config,
            }),
            Box::new(TopicReady),
        ]);

        let observed_generation = ctx.app.metadata.generation;
        let mut original_app = ctx.app.clone();
        let conditions = ctx
            .app
            .section::<KafkaAppStatus>()
            .and_then(|s| s.ok())
            .unwrap_or_default()
            .conditions;

        let result = match constructor.run(conditions, ctx).await {
            Construction::Complete(mut context, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Complete);
                context
                    .app
                    .finish_ready::<KafkaAppStatus>(conditions, observed_generation)?;
                ProcessOutcome::Complete(context.app)
            }
            Construction::Retry(mut context, when, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Progressing);
                context
                    .app
                    .finish_ready::<KafkaAppStatus>(conditions, observed_generation)?;
                ProcessOutcome::Retry(context.app, when)
            }
            Construction::Failed(err, mut conditions) => {
                conditions.update(CONDITION_RECONCILED, ReadyState::Failed(err.to_string()));
                original_app.finish_ready::<KafkaAppStatus>(conditions, observed_generation)?;
                match err {
                    ReconcileError::Permanent(_) => ProcessOutcome::Complete(original_app),
                    ReconcileError::Temporary(_) => ProcessOutcome::Retry(original_app, None),
                }
            }
        };

        // done

        Ok(result)
    }

    async fn deconstruct(
        &self,
        mut ctx: Self::Deconstruct,
    ) -> Result<ProcessOutcome<Self::Output>, ReconcileError> {
        // delete

        self.delete_kafka_topic(EventTarget::Events(ctx.app.metadata.name.clone()))
            .await?;

        // remove finalizer

        ctx.app.metadata.finalizers.retain(|f| f != FINALIZER);

        // done

        Ok(ProcessOutcome::Complete(ctx.app))
    }
}

struct CreateTopic<'o> {
    pub api: &'o Api<DynamicObject>,
    pub resource: &'o ApiResource,
    pub config: &'o ControllerConfig,
}

#[async_trait]
impl<'o> ConstructOperation<ConstructContext> for CreateTopic<'o> {
    fn type_name(&self) -> String {
        "CreateTopics".into()
    }

    async fn run(
        &self,
        mut ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::construct::Result<ConstructContext>
    {
        let topic = ApplicationReconciler::ensure_kafka_topic(
            &self.api,
            &self.resource,
            &self.config,
            EventTarget::Events(ctx.app.metadata.name.clone()),
        )
        .await?;

        ctx.events_topic = Some(topic);

        // done

        Ok(Outcome::Continue(ctx))
    }
}

struct TopicReady;

#[async_trait]
impl ConstructOperation<ConstructContext> for TopicReady {
    fn type_name(&self) -> String {
        "TopicsReady".into()
    }

    async fn run(&self, ctx: ConstructContext) -> construct::Result<ConstructContext> {
        let events_ready = ctx
            .events_topic
            .as_ref()
            .and_then(|topic| Self::is_topic_ready(topic))
            .unwrap_or_default();

        match events_ready {
            true => {

                ctx.app.set_section(Kafka)

                Ok(Outcome::Continue(ctx))
            },
            false => Self::retry(ctx),
        }
    }
}

impl TopicReady {
    fn retry<C>(ctx: C) -> construct::Result<C>
    where
        C: Send + Sync,
    {
        Ok(Outcome::Retry(ctx, Some(Duration::from_secs(15))))
    }

    fn is_topic_ready(topic: &DynamicObject) -> Option<bool> {
        topic.data["status"]["conditions"]
            .as_array()
            .and_then(|conditions| {
                conditions
                    .iter()
                    .filter_map(|cond| cond.as_object())
                    .filter_map(|cond| {
                        if cond["type"] == "Ready" {
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
}

impl<'a> ApplicationReconciler<'a> {
    async fn ensure_kafka_topic(
        kafka_topics: &Api<DynamicObject>,
        kafka_topic_resource: &ApiResource,
        config: &ControllerConfig,
        target: EventTarget,
    ) -> Result<DynamicObject, ReconcileError> {
        let topic_name = make_topic_resource_name(target.clone());

        let topic = create_or_update_by(
            &kafka_topics,
            Some(config.topic_namespace.clone()),
            &topic_name,
            |meta| {
                let mut topic = DynamicObject::new(&topic_name, &kafka_topic_resource)
                    .within(&config.topic_namespace);
                *topic.meta_mut() = meta;
                topic
            },
            |this, that| this.metadata == that.metadata && this.data == that.data,
            |mut topic| {
                // set target cluster
                topic
                    .metadata
                    .labels
                    .insert(LABEL_KAFKA_CLUSTER.into(), config.cluster_name.clone());
                topic
                    .metadata
                    .annotations
                    .insert(ANNOTATION_APP_NAME.into(), target.app_name().into());
                // set config
                topic.data["spec"] = json!({
                    "config": {},
                    "partitions": 3,
                    "replicas": 1,
                    "topicName": topic_name,
                });

                Ok::<_, ReconcileError>(topic)
            },
        )
        .await?
        .resource();

        // done

        Ok(topic)
    }

    async fn delete_kafka_topic(&self, target: EventTarget) -> Result<(), ReconcileError> {
        let topic_name = make_topic_resource_name(target);

        // remove topic

        self.kafka_topics
            .delete_optionally(&topic_name, &Default::default())
            .await?;

        // done

        Ok(())
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
