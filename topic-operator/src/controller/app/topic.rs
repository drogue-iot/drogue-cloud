use super::{condition_ready, retry, ConstructContext, ANNOTATION_APP_NAME, LABEL_KAFKA_CLUSTER};
use crate::controller::ControllerConfig;
use async_trait::async_trait;
use drogue_client::{registry::v1::KafkaAppStatus, Translator};
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{self, OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use kube::{
    api::{ApiResource, DynamicObject},
    Api, Resource,
};
use operator_framework::process::create_or_update_by;
use serde_json::json;

pub struct CreateTopic<'o> {
    pub api: &'o Api<DynamicObject>,
    pub resource: &'o ApiResource,
    pub config: &'o ControllerConfig,
}

impl CreateTopic<'_> {
    async fn ensure_kafka_topic(
        kafka_topics: &Api<DynamicObject>,
        kafka_topic_resource: &ApiResource,
        config: &ControllerConfig,
        target: ResourceType,
    ) -> Result<(DynamicObject, String), ReconcileError> {
        let topic_name = make_kafka_resource_name(target.clone());

        let topic = create_or_update_by(
            kafka_topics,
            Some(config.topic_namespace.clone()),
            &topic_name,
            |meta| {
                let mut topic = DynamicObject::new(&topic_name, kafka_topic_resource)
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

        Ok((topic, topic_name))
    }
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateTopic<'o> {
    fn type_name(&self) -> String {
        "CreateTopics".into()
    }

    async fn run(
        &self,
        mut ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let (topic, topic_name) = Self::ensure_kafka_topic(
            self.api,
            self.resource,
            self.config,
            ResourceType::Events(ctx.app.metadata.name.clone()),
        )
        .await?;

        ctx.events_topic = Some(topic);
        ctx.events_topic_name = Some(topic_name);

        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}

pub struct TopicReady<'o> {
    pub config: &'o ControllerConfig,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for TopicReady<'o> {
    fn type_name(&self) -> String {
        "TopicsReady".into()
    }

    async fn run(&self, mut ctx: ConstructContext) -> progress::Result<ConstructContext> {
        let events_ready = ctx
            .events_topic
            .as_ref()
            .and_then(|topic| condition_ready("Ready", topic))
            .unwrap_or_default();

        ctx.app.update_section(|mut status: KafkaAppStatus| {
            // using the internal model only for now
            status.downstream = None;
            status
        })?;

        match events_ready {
            true => Ok(OperationOutcome::Continue(ctx)),
            false => retry(ctx),
        }
    }
}
