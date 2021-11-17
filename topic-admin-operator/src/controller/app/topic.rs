use super::ConstructContext;
use crate::{controller::ControllerConfig, kafka::TopicErrorConverter};
use async_trait::async_trait;
use drogue_cloud_operator_common::controller::reconciler::{
    progress::{OperationOutcome, ProgressOperation},
    ReconcileError,
};
use drogue_cloud_service_api::kafka::{make_kafka_resource_name, ResourceType};
use rdkafka::{
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    error::{KafkaError, RDKafkaErrorCode},
};

pub struct CreateTopic<'o> {
    pub config: &'o ControllerConfig,
    pub admin: &'o AdminClient<DefaultClientContext>,
}

#[async_trait]
impl<'o> ProgressOperation<ConstructContext> for CreateTopic<'o> {
    fn type_name(&self) -> String {
        "CreateTopics".into()
    }

    async fn run(
        &self,
        ctx: ConstructContext,
    ) -> drogue_cloud_operator_common::controller::reconciler::progress::Result<ConstructContext>
    {
        let topic_name =
            make_kafka_resource_name(ResourceType::Events(ctx.app.metadata.name.clone()));

        let mut config = Vec::with_capacity(self.config.properties.len());
        for (k, v) in &self.config.properties {
            config.push((k.as_str(), v.as_str()));
        }

        let topic = NewTopic {
            name: &topic_name,
            num_partitions: self
                .config
                .num_partitions
                .get()
                .try_into()
                .unwrap_or(i32::MAX),
            config,
            replication: TopicReplication::Fixed(
                self.config
                    .num_replicas
                    .get()
                    .try_into()
                    .unwrap_or(i32::MAX),
            ),
        };

        match self
            .admin
            .create_topics(&[topic], &AdminOptions::new())
            .await
            .single_topic_response()
        {
            Ok(_) => {
                log::debug!("Topic {} created", topic_name);
            }
            Err(KafkaError::AdminOp(RDKafkaErrorCode::TopicAlreadyExists)) => {
                log::debug!("Topic {} already existed", topic_name);
            }
            Err(err) => {
                log::warn!("Failed to create topic ({}): {:?}", topic_name, err);
                return Err(ReconcileError::permanent(format!(
                    "Failed to create topic: {}",
                    err
                )));
            }
        }

        // done

        Ok(OperationOutcome::Continue(ctx))
    }
}
