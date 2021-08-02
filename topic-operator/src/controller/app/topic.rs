use super::{condition_ready, retry, ApplicationReconciler, ConstructContext};
use crate::{controller::ControllerConfig, data::*};
use async_trait::async_trait;
use drogue_client::Translator;
use drogue_cloud_operator_common::controller::reconciler::progress::{
    self, OperationOutcome, ProgressOperation,
};
use drogue_cloud_service_common::kafka::ResourceType;
use kube::{
    api::{ApiResource, DynamicObject},
    Api,
};

pub struct CreateTopic<'o> {
    pub api: &'o Api<DynamicObject>,
    pub resource: &'o ApiResource,
    pub config: &'o ControllerConfig,
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
        let (topic, topic_name) = ApplicationReconciler::ensure_kafka_topic(
            &self.api,
            &self.resource,
            &self.config,
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
