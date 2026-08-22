//! Secret-safe local logging and optional bounded OTLP trace export.

#[cfg(feature = "otlp")]
use std::time::Duration;

use node::{LogFormat, NodeError, TelemetryConfig};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "otlp")]
const OTLP_EXPORT_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) struct TelemetryGuard {
    #[cfg(feature = "otlp")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl TelemetryGuard {
    pub(crate) fn initialize(
        config: &TelemetryConfig,
        json_override: bool,
    ) -> Result<Self, NodeError> {
        let filter =
            EnvFilter::try_new(&config.log_filter).map_err(|_| NodeError::Configuration)?;
        let json = json_override || config.log_format == LogFormat::Json;

        #[cfg(feature = "otlp")]
        {
            let provider = config
                .otlp_endpoint
                .as_deref()
                .filter(|endpoint| !endpoint.is_empty())
                .map(|endpoint| build_provider(endpoint, config.otlp_sample_ratio))
                .transpose()?;
            let telemetry_layer = provider.as_ref().map(|provider| {
                use opentelemetry::trace::TracerProvider as _;

                tracing_opentelemetry::layer().with_tracer(provider.tracer("envshare-node"))
            });
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(telemetry_layer);
            if json {
                subscriber
                    .with(json_layer())
                    .try_init()
                    .map_err(|_| NodeError::Configuration)?;
            } else {
                subscriber
                    .with(text_layer())
                    .try_init()
                    .map_err(|_| NodeError::Configuration)?;
            }
            Ok(Self { provider })
        }

        #[cfg(not(feature = "otlp"))]
        {
            if config
                .otlp_endpoint
                .as_deref()
                .is_some_and(|endpoint| !endpoint.is_empty())
            {
                return Err(NodeError::Configuration);
            }
            let subscriber = tracing_subscriber::registry().with(filter);
            if json {
                subscriber
                    .with(json_layer())
                    .try_init()
                    .map_err(|_| NodeError::Configuration)?;
            } else {
                subscriber
                    .with(text_layer())
                    .try_init()
                    .map_err(|_| NodeError::Configuration)?;
            }
            Ok(Self {})
        }
    }
}

#[cfg(feature = "otlp")]
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.provider {
            let _ = provider.shutdown_with_timeout(OTLP_EXPORT_TIMEOUT);
        }
    }
}

fn json_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    tracing_subscriber::fmt::layer()
        .json()
        .with_target(false)
        .with_writer(std::io::stderr)
}

fn text_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    tracing_subscriber::fmt::layer()
        .compact()
        .with_target(false)
        .with_writer(std::io::stderr)
}

#[cfg(feature = "otlp")]
fn build_provider(
    endpoint: &str,
    sample_ratio: f64,
) -> Result<opentelemetry_sdk::trace::SdkTracerProvider, NodeError> {
    use opentelemetry_otlp::WithExportConfig as _;
    use opentelemetry_sdk::{
        Resource,
        trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider},
    };

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_timeout(OTLP_EXPORT_TIMEOUT)
        .build()
        .map_err(|_| NodeError::Configuration)?;
    let batch = BatchConfigBuilder::default()
        .with_max_queue_size(256)
        .with_max_export_batch_size(64)
        .with_scheduled_delay(Duration::from_secs(5))
        .build();
    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(batch)
        .build();
    Ok(SdkTracerProvider::builder()
        .with_sampler(Sampler::TraceIdRatioBased(sample_ratio))
        .with_max_attributes_per_span(16)
        .with_max_events_per_span(32)
        .with_max_links_per_span(0)
        .with_resource(
            Resource::builder()
                .with_service_name("envshare-node")
                .build(),
        )
        .with_span_processor(processor)
        .build())
}
