#[cfg(feature = "otel")]
use opentelemetry::global;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use tracing_opentelemetry::OpenTelemetryLayer;
#[cfg(feature = "otel")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "otel")]
pub fn init_otel() {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "zoey".to_string());

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .ok();
    if let Some(tracer) = tracer {
        let layer = OpenTelemetryLayer::new(tracer);
        let fmt_layer = tracing_subscriber::fmt::layer();
        let env = tracing_subscriber::EnvFilter::from_default_env();
        tracing_subscriber::registry()
            .with(env)
            .with(fmt_layer)
            .with(layer)
            .init();
        let _ = global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );
    }
}

#[cfg(not(feature = "otel"))]
pub fn init_otel() {}
