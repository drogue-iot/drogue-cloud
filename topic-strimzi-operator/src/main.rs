use drogue_cloud_service_api::PROJECT;
use drogue_cloud_service_common::runtime;
use drogue_cloud_topic_strimzi_operator::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    runtime!(PROJECT).exec(run).await
}
