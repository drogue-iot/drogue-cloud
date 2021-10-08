use drogue_cloud_console_backend::{run, Config};
use drogue_cloud_service_common::{config::ConfigFromEnv, endpoints::create_endpoint_source};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::from_env().unwrap();

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;
    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoints = endpoint_source.eval_endpoints().await?;

    run(config, endpoints).await
}
