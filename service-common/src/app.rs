use crate::health::{HealthServer, HealthServerConfig};
use drogue_cloud_service_api::health::HealthChecked;
use futures::FutureExt;
use futures_util::select;
use std::future::Future;

#[macro_export]
macro_rules! app {
    () => {
        $crate::main!(run(Config::from_env()?).await)
    };
}

#[macro_export]
macro_rules! main {
    ($run:expr) => {{

        const VERSION: &str = env!("CARGO_PKG_VERSION");
        const NAME: &str = env!("CARGO_PKG_NAME");
        const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

        use drogue_cloud_service_common::config::ConfigFromEnv;
        dotenv::dotenv().ok();

        println!(r#"______ ______  _____  _____  _   _  _____   _____         _____ 
|  _  \| ___ \|  _  ||  __ \| | | ||  ___| |_   _|       |_   _|
| | | || |_/ /| | | || |  \/| | | || |__     | |    ___    | |  
| | | ||    / | | | || | __ | | | ||  __|    | |   / _ \   | |  
| |/ / | |\ \ \ \_/ /| |_\ \| |_| || |___   _| |_ | (_) |  | |  
|___/  \_| \_| \___/  \____/ \___/ \____/   \___/  \___/   \_/  
Drogue IoT {} - {} {} ({})
"#, drogue_cloud_service_api::version::VERSION, NAME, VERSION, DESCRIPTION);

        $crate::app::init_tracing(NAME);

        return $run;
    }};
}

#[cfg(feature = "jaeger")]
fn enable_tracing() -> bool {
    std::env::var("ENABLE_TRACING")
        .ok()
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or_default()
}

#[cfg(feature = "jaeger")]
pub fn init_tracing(name: &str) {
    if !enable_tracing() {
        init_no_tracing();
        return;
    }

    use crate::tracing::{self, tracing_subscriber::prelude::*};

    tracing::opentelemetry::global::set_text_map_propagator(
        tracing::opentelemetry::sdk::propagation::TraceContextPropagator::new(),
    );
    let pipeline = tracing::opentelemetry_jaeger::new_pipeline()
        .with_service_name(name)
        .with_trace_config(
            tracing::opentelemetry::sdk::trace::Config::default().with_sampler(
                opentelemetry::sdk::trace::Sampler::ParentBased(Box::new(tracing::sampler())),
            ),
        );

    println!("Using Jaeger tracing.");
    println!("{:#?}", pipeline);
    println!("Tracing is enabled. This console will not show any logging information.");

    let tracer = pipeline
        .install_batch(tracing::opentelemetry::runtime::Tokio)
        .unwrap();

    tracing::tracing_subscriber::Registry::default()
        .with(tracing::tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing::tracing_opentelemetry::layer().with_tracer(tracer))
        .init();
}

#[cfg(not(feature = "jaeger"))]
#[allow(unused)]
fn init_tracing(_: &str) {
    init_no_tracing()
}

#[allow(unused)]
fn init_no_tracing() {
    env_logger::init();
    log::info!("Tracing is not enabled");
}

/// Run a standard main loop.
pub async fn run_main(
    main: impl Future<Output = Result<(), impl Into<anyhow::Error> + Send + Sync + 'static>>,
    health: Option<HealthServerConfig>,
    checks: Vec<Box<dyn HealthChecked>>,
) -> anyhow::Result<()> {
    if let Some(health) = health {
        let health =
            HealthServer::new(health, checks, Some(prometheus::default_registry().clone()));

        select! {
            _ = health.run().fuse() => (),
            _ = main.fuse() => (),
        }
    } else {
        main.await.map_err(|err| err.into())?;
    }

    Ok(())
}
