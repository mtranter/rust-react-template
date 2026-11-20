# Traces and metrics are exported over OTLP, and a trace crosses the outbox in the Event Envelope

Spec 0003. Before it, the app had JSON logs and a `requestId` and nothing else: a request that published an Event lost its trail at the outbox, nothing measured latency, errors or the queues, and an agent holding a failed tool result had nowhere to look its `requestId` up.

**Every binary exports over OTLP, configured only by the standard `OTEL_*` variables.** `server::telemetry::init(env, "<binary>")` (`server`, `worker`, `migrate`, `seed`) installs a `tracing-opentelemetry` layer and a meter provider beside the log layer, and returns a `Telemetry` the binary holds until it exits: dropping it flushes and shuts both pipelines down. `OTEL_EXPORTER_OTLP_ENDPOINT` turns export on; unset or empty, nothing is exported, no exporter is built, and behaviour is as before (log lines carry no `trace_id`). `OTEL_EXPORTER_OTLP_PROTOCOL` (`http/protobuf` by default, or `http/json`), `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME` (defaulting to the binary's name), `OTEL_RESOURCE_ATTRIBUTES`, `OTEL_TRACES_SAMPLER`/`_ARG` and `OTEL_METRIC_EXPORT_INTERVAL` are read as the specification defines them, and `.env.example` lists them. The app's name and version are the resource's `service.namespace` and `service.version`. So the backend (a collector, AWS X-Ray and CloudWatch, Grafana Cloud, `make dev-observability`'s local Grafana) is a deployment's choice, never a code change, and there is no config key of the app's own for any of it.

**Exporting never blocks a request.** Spans go through the SDK's batch processor and metrics through its periodic reader, each exporting from a thread of its own over a blocking `reqwest` 0.12 client (the workspace's), so a slow or unreachable collector fills the span queue and drops spans rather than holding a request up. The SDK's own warnings (a dropped span, a failed export) are let through at most once a minute per kind. The exporter's own HTTP calls are filtered out of the traces. Recording a metric only adds to an aggregate in memory.

**The sampler is parent-based, always on, by default.** The SDK's default, read from `OTEL_TRACES_SAMPLER`: a caller's sampling decision in its `traceparent` is honoured, and a root is always sampled. A deployment that finds the volume too high lowers it with the standard variables (`parentbased_traceidratio` and an `OTEL_TRACES_SAMPLER_ARG`), not a code change. Tail-based sampling is a collector's job, out of scope here.

**What is spanned.** Code spans with `tracing`, never the `opentelemetry` API, so every `tracing` span is an OpenTelemetry span and a module needs no new dependency:

- each HTTP request (`app_web::TraceRequests`), named by its route template, with `operation_id`, the status, `request_id` and the principal's subject, continuing an incoming W3C `traceparent` header so a calling agent's trace continues into the app;
- each MCP tool call, named by the tool, with its outcome (`is_error`), the parent of its in-process REST call's span (ADR 0034);
- each SQL query made inside a span (`app_web::QuerySpans`, built from the event sqlx emits as a query finishes), with the statement and its `$n` placeholders in `db.query.text`, never their bound values;
- in the worker, each run of a job (queue, event type, key, version, attempt), relay batch, backfill chunk, due schedule and compaction run.

Log lines carry the current span's `trace_id` and `span_id`, and `ErrorBody` carries the trace's id as `traceId` beside `requestId` whenever a trace is recorded (amending ADR 0022), as the MCP instructions tell an agent, so a failed REST call, tool result or log line leads straight to its trace.

**A trace crosses the outbox in the Event Envelope.** `PgEventPublisher` records the current span's W3C trace context in each Event's envelope as CloudEvents' distributed-tracing extension attributes, `traceparent` and `tracestate` (the latter only when a vendor set one), beside `topic`, `partitionkey` and `entityversion` (ADR 0012). The worker makes its span for each run of a job a child of the span that published the Event (`crates/common/db/src/trace.rs`), so one trace shows the request and every reaction to it, through any chain of reactions, and each retry is a sibling span in the same trace. An Event published outside any trace (seeds, export off), or before envelopes carried one, has neither attribute, and its handler's span is the root of a new trace. A Deferred Job carries no trace context: each run is a root of its own (ADR 0043). The handler reads nothing of it: continuing the trace is the worker's, not a module's.

**The metric set is fixed, and follows OpenTelemetry's semantic conventions where one exists:**

| Area | Metric | Attributes |
| --- | --- | --- |
| HTTP | `http.server.request.duration` (histogram, `s`) | `operation_id`, `http.request.method`, `http.route`, `http.response.status_code` |
| MCP | `mcp.server.tool.calls` | `gen_ai.tool.name`, `outcome` (`ok`, `error`) |
| Idempotency | `idempotency.replays`, `idempotency.reuse_refusals` | |
| Database pool | `db.client.connection.count` (by `db.client.connection.state`: `used`, `idle`), `db.client.connection.max`, `db.client.connection.wait_time` (histogram, `s`) | `db.client.connection.pool.name` |
| Queues | `messaging.job.attempts` (histogram, at each run), and per queue the gauges `messaging.queue.jobs.due`, `messaging.queue.jobs.waiting`, `messaging.queue.oldest_due.age`, `messaging.queue.dead_letters` | `messaging.destination.name` |
| Relay | `messaging.relay.undispatched`, `messaging.relay.oldest_undispatched.age` | |
| Backfill | `messaging.backfill.remaining`, per subscription | |

Request rates, error rates, `409`s and `412`s are derived from the HTTP histogram rather than counted apart. The queue, relay and backfill gauges are read from Postgres every 15 seconds by the one worker holding the `figures` advisory lock, so replicas never report one figure twice; `worker --drain` reads none. A metric's attributes are small closed sets, never an id, a path, a subject or anything a client chooses, since each distinct set is a series kept for the life of the process.

## Considered options

- **An app config section for the exporter (endpoint, headers, sampling ratio) in `figment`.** Every OpenTelemetry SDK, collector, sidecar and cloud integration already speaks the `OTEL_*` variables; a section of our own would be one more thing to map them onto and to document, and would drift.
- **Prometheus scraping for metrics.** A `/metrics` endpoint per instance needs service discovery, sits beside the API on the one binary (ADR 0005) and can't reach a `worker --drain` that has already exited. OTLP pushes from every binary, and a collector can still expose Prometheus.
- **OTLP over gRPC.** It would bring `tonic` and its TLS stack into the one binary for a choice every collector also serves over HTTP. Only `http/protobuf` and `http/json` are built; another protocol is refused at start-up with a warning, exporting nothing.
- **opentelemetry-otlp's own `reqwest` client.** It brings `reqwest` 0.13 and its `aws-lc-rs`; the workspace's 0.12, on a thread of its own, does the same job.
- **Spanning with the `opentelemetry` API directly.** Modules would depend on the SDK's churn (ADR 0026); `tracing` was already everywhere and its spans become OpenTelemetry's through one layer.
- **Trace context in an outbox column, or a header beside the envelope.** The envelope is what travels: a relay to an external broker for a lifted-out Module (ADR 0011) carries it whole, and CloudEvents already defines the extension, so any CloudEvents consumer understands it.
- **The handler's span linked to the publisher's rather than a child of it.** A link keeps traces short but makes "the request succeeded, the email never went out" a search across traces; a child puts the request and its reactions in one. A long chain of reactions makes a long trace, which is the point.
- **Always-on sampling without the parent.** A calling agent that sampled its trace out would still get ours; parent-based keeps one decision per trace.
- **SQL spans opt-in.** Each query is a span, and a list or a backfill makes many. They are the first place to look when a request is slow, so they are on; a query outside any span (an idle worker's poll) records none.

## Consequences

- The backend, its credentials and its sampling are a deployment's environment, never the code's; turning export off is unsetting one variable.
- **SQL span volume** can be large, and is the first cost to watch. If it costs too much, the deployment samples lower; if that isn't enough, `QuerySpans` becomes opt-in.
- **Lambda.** If Lambda becomes a host (Spec 0010), its batch processor and periodic reader won't run while an invocation is frozen, so its handler must call `Telemetry::flush()` at the end of each invocation, which exports what is buffered and keeps the pipelines running. Nothing here rules that out.
- Because `db.query.text` exports a statement as written, a value written into SQL text rather than bound (`QueryBuilder::push`, `format!`) would be exported: always bind.
- `RUST_LOG` filters each layer on its own, not the registry, so sqlx's query events reach `QuerySpans` at `DEBUG` without being logged.
- An instrument records into the meter provider that was global when it was built, so instruments are built in the constructors of what records into them, after `init`, never in a `static`.
- A new metric or span attribute is a code change and a line in the `add-telemetry` skill; the set above is what dashboards and the alerts of the operator surface (Spec 0005) may rely on, and renaming one breaks them.
- The `opentelemetry` crates and `tracing-opentelemetry` move as a pinned pair (ADR 0026).
- The envelope gained two optional attributes, so Events already in the outbox, and those published without a trace, still decode, and their handlers start a trace of their own.
