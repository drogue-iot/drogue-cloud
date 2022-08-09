use drogue_cloud_coap_endpoint::run;
use drogue_cloud_service_api::PROJECT;
use drogue_cloud_service_common::runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    runtime!(PROJECT).exec(run).await
}
