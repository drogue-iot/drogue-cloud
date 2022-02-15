pub use opentelemetry;
use opentelemetry::sdk::trace::Sampler;
pub use tracing_opentelemetry;
pub use tracing_subscriber;

#[cfg(feature = "jaeger")]
pub use opentelemetry_jaeger;

/// Try getting the sampling rate from the environment variables
fn sampling_from_env() -> Option<f64> {
    std::env::var_os("OTEL_TRACES_SAMPLER_ARG")
        .and_then(|s| s.to_str().map(|s| s.parse::<f64>().ok()).unwrap())
}

pub fn sampler() -> Sampler {
    if let Some(p) = sampling_from_env() {
        Sampler::TraceIdRatioBased(p)
    } else {
        Sampler::TraceIdRatioBased(0.001)
    }
}
