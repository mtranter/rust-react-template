---
name: add-telemetry
description: Add or change a trace span or a metric in the Rust backend (tracing, OpenTelemetry, OTLP, Grafana), test one, or explain what the server and worker already span and measure. Use before adding `#[tracing::instrument]`, `info_span!`, a meter or instrument, or when asked what is traced or measured.
---

# Traces and metrics (Spec 0003)

Extends `crates/AGENTS.md`; if this file and the code disagree, the code wins: fix this file in the same change.

## What is already there

Each binary holds `server::telemetry::init(env, "<binary>")` until it exits, called before it connects the pool or builds the app: dropping it flushes and stops the trace and metric pipelines, and `Telemetry::flush()` flushes without stopping. It is configured only by the standard `OTEL_*` variables (`.env.example`); with no `OTEL_EXPORTER_OTLP_ENDPOINT` nothing is exported and log lines carry no `trace_id`. Locally, `make dev-observability` exports to a Grafana at `:3000`.

- Every HTTP request is already one span (`app_web::TraceRequests`), named by its route template (`DELETE /api/access/roles/{name}`), continuing a caller's `traceparent`.
- Each MCP tool call is one span too, named by the tool (`delete_role`), with `is_error` (the result's `isError`; unset, with error status, when the call is refused before any request) and `gen_ai.tool.name`, a name no tool has spanned as `tools/call`, the parent of its in-process REST call's span (`app_web::mcp`, which wraps the API it calls in `TraceRequests` of its own).
- A trace crosses the outbox: `PgEventPublisher` records the current span's W3C context in the envelope (`traceparent`/`tracestate`, CloudEvents' distributed-tracing extension; none outside a trace, as in seeds), and the worker's span for each run of a job (`job`: queue, event type, key, version, attempt) is its child, each retry a sibling, or a new trace's root when the envelope has none (`crates/common/db/src/trace.rs`).
- The worker also spans each `relay_batch` (only when there is one), `backfill_chunk`, `schedule_slots` (only when a schedule is due) and `compaction`.
- Each SQL query made inside a span, on the pool, a connection or a `Tx`, is a client span under it, named by the statement's first keyword (`SELECT`), with `db.system.name` and `db.query.text`, the statement with its `$1` placeholders and never their values: `app_web::QuerySpans` builds it from the `sqlx::query` event sqlx emits as a query finishes, so a query outside any span (an idle worker's poll) records none. So `RUST_LOG` filters each layer, not the registry, and never hides those events from it; and a value written into SQL text rather than bound (`QueryBuilder::push`, `format!`) would be exported: always bind.

Metrics go to the global meter provider `init` installs (a no-op one when nothing is exported), read by a periodic reader on a thread of its own, so recording one only adds to an aggregate in memory. Measured today:

- `http.server.request.duration` (`TraceRequests`, by `operation_id`, `http.request.method`, `http.route`, `http.response.status_code`), `mcp.server.tool.calls` (by `gen_ai.tool.name` and `outcome`), `idempotency.replays` and `idempotency.reuse_refusals`.
- `db.client.connection.count` / `.max` (`app_db::observe_pool`, which `app_db::connect` calls) and `db.client.connection.wait_time` (`app_web::PoolWaits`, from the `sqlx::pool::acquire` event a pool built with an `acquire_time_level` emits).
- In the worker (`crates/common/db/src/figures.rs`): `messaging.job.attempts` (a histogram recorded at each run of a job, by `messaging.destination.name`), and gauges read from Postgres every 15 s (`Worker::run`, `Worker::read_figures`) by the one worker holding the `figures` session advisory lock on a connection detached from its pool, until that connection closes: per queue `messaging.queue.jobs.due`, `.jobs.waiting` (a running job counts as waiting: its claim pushed `run_at` past the lease), `.oldest_due.age` and `.dead_letters`; `messaging.relay.undispatched` and `.oldest_undispatched.age`; and `messaging.backfill.remaining` per subscription. Every other worker observes none of them, so replicas never report the same figure twice; a failed read is logged, exports nothing until the next succeeds and lets the lock go. `worker --drain` doesn't read them; only `run` does.

Only these are `app_web`'s and `app_db`'s: the request's span, the query spans, the metrics measured where requests are served, and the worker's delivery figures. The pipeline itself lives in `crates/server`.

## Adding a span

- Use `tracing`, never the `opentelemetry` API: every `tracing` span becomes an OpenTelemetry span, a child of the current one, and its fields its attributes. `#[tracing::instrument(skip_all, fields(note_id = %id))]` on a function, or `let span = tracing::info_span!("relay_batch", queue = %queue);` then `.instrument(span)` on a future (never hold `span.enter()` across an `.await`).
- A value known only later is declared `field::Empty` and recorded with `span.record("rows", n)`; a field not declared up front is dropped. Use OpenTelemetry's semantic-convention name where one exists (`http.route`, `db.query.text`), and `otel.kind` / `otel.status_code = "error"` for the span's kind and status. `otel.name` renames a span only before it starts (first entered); after, `span.context().span().update_name(..)` (`tracing_opentelemetry::OpenTelemetrySpanExt`).
- `skip_all` and name each field: never a token, a secret, a bound SQL value or personal data beyond the principal's subject.
- Test it with `app_web::testing::Traces::install()` (the `test` feature), which captures this thread's spans in memory, then `traces.span(name)` and `Traces::attribute(&span, key)`: copy `a_request_is_one_span_with_its_operation_continuing_the_callers_trace` in `crates/server/tests/composed_app.rs`, or for the worker `crates/common/db/tests/delivery/traces.rs`.

## Adding a metric

Copy `crates/common/web/src/metrics.rs`.

- Build the instrument from `opentelemetry::global::meter("<crate>")` in the constructor of what records into it (a middleware, a service), never in a `static` or `LazyLock`: an instrument records into the provider that was global when it was built, so one built before `init` records nothing, ever. A gauge of state (a pool, a queue) is an observable instrument whose callback reads it at each collection.
- Use OpenTelemetry's semantic-convention name, unit (`s` for durations, as an `f64` histogram; `{request}` for a count) and attribute names where one exists; otherwise a dotted name of the area (`idempotency.replays`). Keep attributes to small closed sets (an operation id, a status, a tool name), never an id, a path, a subject or anything a client chooses: each distinct set is a series kept for the life of the process.
- Test it with `app_web::testing::Metrics::install()` before building the app, then `metrics.names()` after exercising it: copy `every_request_side_metric_is_recorded` in `crates/server/tests/composed_app.rs`, or for the worker `every_delivery_metric_is_registered` in `crates/common/db/tests/delivery/figures.rs`. The provider is global, shared by every test of the binary, so check a metric is there, not its value.
