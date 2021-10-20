use dotenv::dotenv;
use drogue_cloud_service_common::config::ConfigFromEnv;
use drogue_cloud_ttn_operator::{run, Config};

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    run(Config::from_env()?).await
}
