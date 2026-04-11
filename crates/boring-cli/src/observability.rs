use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Initialize observability: structured JSON logging + optional OTel tracing.
///
/// OTel is enabled when the `otel` feature is active AND the `OTEL_EXPORTER_OTLP_ENDPOINT`
/// env var is set. Otherwise, just structured logging.
pub fn init() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(false);

    #[cfg(feature = "otel")]
    {
        if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
            if let Ok(tracer) = init_otel_tracer() {
                let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .with(otel_layer)
                    .init();
                tracing::info!("OTel tracing enabled");
                return;
            }
        }
    }

    // Fallback: just structured logging
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

#[cfg(feature = "otel")]
fn init_otel_tracer() -> Result<opentelemetry_sdk::trace::Tracer, Box<dyn std::error::Error>> {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::SpanExporter;

    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    let tracer = provider.tracer("borechestrator");
    opentelemetry::global::set_tracer_provider(provider);
    Ok(tracer)
}
