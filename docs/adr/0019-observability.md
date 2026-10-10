# 0019: Metrics and traces

- Status: accepted · 2026-10-10

## Context
Self-hosters want to see whether the queue keeps up, why a search is slow and what the
language model costs, in the tools they already run (Prometheus + Grafana, an
OpenTelemetry collector). `/api/v1/system/status` answers some of this for one user in
the UI but is not scrapeable over time.

## Decision
- **Prometheus metrics** through the `metrics` facade and `metrics-exporter-prometheus`
  (no HTTP listener feature; we serve the text ourselves). Off unless
  `AKASHA_METRICS_ENABLED=true`; the facade is a no-op without a recorder, so code
  records unconditionally. Metric names and labels are listed in `crates/app/src/metrics`.
  HTTP metrics use the matched route template, never raw paths (bounded cardinality).
  Queue depth and model readiness are gauges refreshed every 15 s from the database and
  the model provider; job outcome/duration/wait are recorded by the job runtime.
- **Exposure**: `AKASHA_METRICS_BIND_ADDR` serves `/metrics` on a separate listener
  without auth (for a private network, e.g. the Compose network); otherwise `/metrics` on
  the main port needs `Authorization: Bearer $AKASHA_METRICS_TOKEN` and `serve` refuses to
  start without a token. Worker-only processes expose metrics on the separate listener.
- **Traces**: OpenTelemetry over OTLP/HTTP (protobuf) via `tracing-opentelemetry` when
  `OTEL_EXPORTER_OTLP_ENDPOINT` is set (standard `OTEL_*` variables apply; service name
  `akasha` unless `OTEL_SERVICE_NAME`). The exporter uses the blocking reqwest client we
  already ship, in its own batch thread. Request spans carry `request_id` (the
  `x-request-id` header) and continue an incoming W3C `traceparent`. Behind cargo feature
  `otel` (on by default) so minimal builds can drop it.
- `deploy/grafana/akasha.json` is a starter dashboard; `compose --profile monitoring`
  runs Prometheus and Grafana with it provisioned.

## Consequences
- HTTP latency is time to response headers: streamed chat answers are measured by the
  LLM metrics instead.
- Token counts are what providers report (Ollama and OpenAI-compatible servers may omit
  them).
