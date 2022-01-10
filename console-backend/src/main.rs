use drogue_cloud_console_backend::{run, Config};
use drogue_cloud_service_common::{endpoints::create_endpoint_source, main};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    main!({
        // the endpoint source we choose
        let endpoint_source = create_endpoint_source()?;
        log::info!("Using endpoint source: {:#?}", endpoint_source);
        let endpoints = endpoint_source.eval_endpoints().await?;

        run(Config::from_env()?, endpoints).await
    });
}
