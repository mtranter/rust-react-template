---
status: ready-for-tickets
---

# Traces and metrics follow every request and event through OpenTelemetry

## Problem

An agent, or a person, diagnosing this app today has JSON logs and a `requestId`, and nothing else. `crates/server/src/telemetry.rs` sets up `tracing-subscriber` only, with the OpenTelemetry hook left as a comment. There are no metrics.

- A request that publishes an event loses its trail at the outbox: the worker's handler has no link back to the request. "The request succeeded but the email never went out" can't be followed.
- Nothing measures how the app behaves: request latency and errors per operation, queue depth, the age of the oldest job, relay lag, dead letters, database pool saturation.
- An agent holding a failed tool result has a `requestId` but nowhere to look it up.

Ops agents (a later step) need all three before they can diagnose anything.

## Solution overview

The server, the worker and `migrate` export traces and metrics over OTLP using the `opentelemetry` crates and `tracing-opentelemetry`. They're configured by the standard `OTEL_*` environment variables, so the backend (AWS X-Ray and CloudWatch, Grafana Cloud, a collector) is a deployment choice, not a code change. With no endpoint set, nothing is exported and behaviour is as today.

- A trace follows a request across the outbox. Each event carries the W3C trace context of the request that published it, as the CloudEvents distributed-tracing extension, and the handler that receives it continues that trace.
- `ErrorBody` gains `traceId`, so a failed REST call or tool result points straight at its trace.
- A fixed set of metrics covers HTTP, MCP, the queues, the relay, the database pool and idempotency.

## Behaviour

### Configuration

- The standard variables: `OTEL_EXPORTER_OTLP_ENDPOINT` (unset turns export off), `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES`, `OTEL_TRACES_SAMPLER` and `OTEL_TRACES_SAMPLER_ARG`.
- `OTEL_SERVICE_NAME` defaults to the binary: `server`, `worker` or `migrate`. The app's name and version are resource attributes.
- The default sampler is parent-based, always on. A deployment lowers it with the standard variables.
- Exporting never blocks a request. A slow or unreachable collector drops spans and logs a warning at most once a minute.
- Each binary flushes its exporters on shutdown.
- `.env.example` documents the variables.

### Traces

- **HTTP:** one span per request, named by route template, with `operation_id`, status, `request_id` and the principal's subject when there is one.
  - An incoming `traceparent` header is honoured, so a calling agent's trace continues into the app.
- **MCP:** one span per tool call, named by the tool, with its outcome (`isError`). Its in-process REST call (ADR 0034) is a child span.
- **SQL:** one span per query, with the statement text but never its bound values.
- **Worker:** spans for each relay batch, backfill chunk, compaction run, and each job run (queue, event type, key, version, attempt).
- **Across the outbox:**
  - Publishing an event records the current trace context in its envelope as the CloudEvents `traceparent` and `tracestate` extension attributes.
  - The handler's span is a child of the publishing span, so one trace shows the request and every reaction to it. Each retry is a sibling span in the same trace.
  - Events already in the outbox, and events published by `System` outside any request (seeds, reactions), carry the trace they were published in, or none; a handler without one starts a new trace.
- Logs carry `trace_id` and `span_id`, so a log line leads to its trace.

### Errors

- `ErrorBody` gains an optional `traceId`, present whenever tracing is on, beside `requestId`. The MCP instructions mention it.
- `make generate` is committed with the change.

### Metrics

Exported over OTLP, following OpenTelemetry's semantic conventions where one exists:

- **HTTP:** request duration as a histogram, by `operation_id`, method and status. Rates, error rates, `409`s and `412`s are derived from it.
- **MCP:** tool calls by tool and outcome.
- **Queues**, per queue:
  - jobs due;
  - jobs waiting for a later `run_at`;
  - the age of the oldest due job;
  - dead letters;
  - attempts, as a histogram.
- **Relay:** undispatched outbox rows, and the age of the oldest.
- **Backfill:** events still to copy, per subscription.
- **Database pool:** connections in use, idle and maximum, and how long acquiring one waits.
- **Idempotency:** replays served, and keys refused for reuse.

The queue, relay and backfill figures are read from Postgres by one worker at a time, every 15 seconds.

### Local development

- An optional docker compose profile, `observability`, runs an all-in-one OTLP backend (for example `grafana/otel-lgtm`). `make dev-observability`, or a documented variable, points `make dev` at it.
- `make dev` without it is unchanged.

### Tests

Tests use an in-memory exporter:

- a request's span carries its `operation_id`;
- an event published in a request is handled in the same trace;
- an `ErrorBody` carries the trace's id;
- every metric listed above is registered.

### Docs

- An ADR records OTLP and the standard variables, trace context carried in the envelope, and the metric set.
- The "OpenTelemetry seam" note in `crates/AGENTS.md` is replaced by how to add a span or a metric.

## Out of scope

- Choosing or provisioning a backend, dashboards and alert rules. Alerts and their runbooks belong with the operator surface (Spec 0005).
- Tracing in the browser.
- Shipping logs anywhere beyond stdout.
- Tail-based sampling.

## Open risks

- **Crate churn.** The `opentelemetry` crates release often and must be pinned in pairs with `tracing-opentelemetry` (ADR 0026).
- **Lambda.** If Lambda becomes a host (Spec 0010), exporters must flush at the end of each invocation. The design here must not rule that out.
- **SQL span volume** can be large. If it costs too much, they become opt-in.
