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
pub fn init_tracing(name: &str) {
    use crate::tracing::{self, tracing_subscriber::prelude::*};

    tracing::opentelemetry::global::set_text_map_propagator(
        tracing::opentelemetry::sdk::propagation::TraceContextPropagator::new(),
    );
    let tracer = tracing::opentelemetry_jaeger::new_pipeline()
        .with_service_name(name)
        .install_batch(tracing::opentelemetry::runtime::Tokio)
        .unwrap();

    tracing::tracing_subscriber::Registry::default()
        .with(tracing::tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    log::info!("Using Jaeger tracing");
}

#[cfg(not(feature = "jaeger"))]
fn init_tracing(_: &str) {
    env_logger::init();
    log::info!("No tracing implementation enabled");
}
