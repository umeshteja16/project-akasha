//! Logging and tracing: `tracing` to stdout (pretty or JSON), plus OpenTelemetry spans
//! over OTLP/HTTP when `OTEL_EXPORTER_OTLP_ENDPOINT` is set (feature `otel`, ADR 0019).

use akasha_core::LogFormat;
use axum::{body::Body, http::Request};
use tracing::Span;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Flushes exported spans when dropped (keep it alive for the life of `main`).
#[derive(Default)]
pub struct Telemetry {
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(provider) = self.provider.take()
            && let Err(err) = provider.shutdown()
        {
            eprintln!("could not flush OpenTelemetry spans: {err}");
        }
    }
}

/// Install the global tracing subscriber. Filter with `RUST_LOG` (default `info`).
pub fn init(format: LogFormat) -> Telemetry {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    #[cfg(feature = "otel")]
    let (otel, telemetry) = match otel::provider() {
        Ok(Some(provider)) => {
            use opentelemetry::trace::TracerProvider as _;
            let layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("akasha"));
            (
                Some(layer),
                Telemetry {
                    provider: Some(provider),
                },
            )
        }
        Ok(None) => (None, Telemetry::default()),
        Err(err) => {
            eprintln!("OpenTelemetry export disabled: {err}");
            (None, Telemetry::default())
        }
    };
    #[cfg(not(feature = "otel"))]
    let (otel, telemetry) = (
        None::<tracing_subscriber::layer::Identity>,
        Telemetry::default(),
    );
    let registry = tracing_subscriber::registry().with(filter).with(otel);
    match format {
        LogFormat::Pretty => registry.with(fmt::layer()).init(),
        LogFormat::Json => registry.with(fmt::layer().json()).init(),
    }
    telemetry
}

/// The span of one HTTP request: method, path (no query string: it may hold search
/// terms) and the `x-request-id` every response returns, so logs, traces and a user's
/// report line up. Continues an incoming W3C `traceparent`.
pub fn request_span(req: &Request<Body>) -> Span {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let span = tracing::info_span!(
        "request",
        method = %req.method(),
        path = %req.uri().path(),
        request_id,
    );
    #[cfg(feature = "otel")]
    otel::continue_trace(&span, req.headers());
    span
}

#[cfg(feature = "otel")]
mod otel {
    use axum::http::HeaderMap;
    use opentelemetry::propagation::Extractor;
    use opentelemetry_sdk::{
        Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider,
    };
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    /// A tracer provider exporting to `OTEL_EXPORTER_OTLP_ENDPOINT`, if set.
    pub fn provider() -> Result<Option<SdkTracerProvider>, String> {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
        let traces = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").unwrap_or_default();
        if endpoint.trim().is_empty() && traces.trim().is_empty() {
            return Ok(None);
        }
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()
            .map_err(|e| e.to_string())?;
        let mut resource = Resource::builder();
        if std::env::var_os("OTEL_SERVICE_NAME").is_none() {
            resource = resource.with_service_name("akasha");
        }
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource.build())
            .build();
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        opentelemetry::global::set_tracer_provider(provider.clone());
        Ok(Some(provider))
    }

    struct Headers<'a>(&'a HeaderMap);

    impl Extractor for Headers<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|v| v.to_str().ok())
        }

        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(|k| k.as_str()).collect()
        }
    }

    pub fn continue_trace(span: &tracing::Span, headers: &HeaderMap) {
        if headers.contains_key("traceparent") {
            let parent =
                opentelemetry::global::get_text_map_propagator(|p| p.extract(&Headers(headers)));
            let _ = span.set_parent(parent);
        }
    }
}
