use drogue_cloud_service_api::kafka::KafkaClientConfig;
use rdkafka::{
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
};

pub async fn create_topics<I, S>(config: KafkaClientConfig, topics: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let admin: rdkafka::ClientConfig = config.into();
    let admin: AdminClient<DefaultClientContext> = admin.create()?;

    for topic in topics {
        log::info!("Pre-creating topic: {}", topic.as_ref());
        admin
            .create_topics(
                [&NewTopic::new(
                    topic.as_ref(),
                    1,
                    TopicReplication::Fixed(1),
                )],
                &AdminOptions::default(),
            )
            .await?;
    }

    Ok(())
}
