use drogue_cloud_mqtt_integration::run;
use drogue_cloud_service_api::PROJECT;
use drogue_cloud_service_common::runtime;

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    runtime!(PROJECT).exec(run).await
}
