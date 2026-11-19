---
id: 0044
name: "The server, worker and migrate export traces over OTLP, with one span per HTTP request"
status: in-progress
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: []
blocks: [docs/platform/tickets/0045-errorbody-carries-the-trace-s-id-as-traceid.md, docs/platform/tickets/0046-a-trace-follows-a-request-across-the-outbox-into-t.md, docs/platform/tickets/0047-each-mcp-tool-call-is-a-span-with-its-outcome-pare.md, docs/platform/tickets/0048-each-sql-query-is-a-span-with-its-statement-but-ne.md, docs/platform/tickets/0049-request-side-metrics-are-exported-over-otlp-http-m.md, docs/platform/tickets/0051-an-optional-observability-compose-profile-runs-a-l.md]
---

# The server, worker and migrate export traces over OTLP, with one span per HTTP request

Replace the OpenTelemetry seam in `server::telemetry` with a real trace pipeline. Use the `opentelemetry` crates, an OTLP exporter and `tracing-opentelemetry`, pinned in pairs as ADR 0026 requires. The pipeline is configured only by the standard `OTEL_*` variables and is off when no endpoint is set. Wrap every HTTP request in a span. This is the foundation the other Tickets for this Spec build on, so give it an in-memory exporter test harness that they can reuse.

Keep the pipeline's flush callable on its own, not only from process exit, so that a later Lambda host (Spec 0010) can flush at the end of each invocation.

## Acceptance criteria
- [ ] The `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` and `tracing-opentelemetry` versions are pinned as a matched set in the workspace `Cargo.toml`, with a comment saying they are bumped together (ADR 0026). `make deny` passes.
- [ ] `OTEL_EXPORTER_OTLP_ENDPOINT` unset means nothing is exported, and logs and behaviour are as today.
- [ ] The pipeline honours `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES`, `OTEL_TRACES_SAMPLER` and `OTEL_TRACES_SAMPLER_ARG`.
- [ ] `OTEL_SERVICE_NAME` defaults to `server`, `worker` or `migrate`, depending on the binary.
- [ ] The app's name and version are resource attributes.
- [ ] The default sampler is parent-based, always on.
- [ ] Spans are exported in batches off the request path, so a slow or unreachable collector never blocks a request.
- [ ] Dropped spans or export failures log a warning at most once a minute.
- [ ] `server`, `worker` and `migrate` each flush their exporters on shutdown.
- [ ] Log lines carry `trace_id` and `span_id` when inside a span, in the JSON (prod) format and the pretty (dev) format.
- [ ] Every HTTP request gets one span, named by its route template. The span carries `operation_id` (for API operations), status, `request_id`, and the principal's subject when there is one.
- [ ] An incoming W3C `traceparent` header is honoured, so the request span continues the caller's trace.
- [ ] There is an in-memory span exporter test helper (e.g. in `app_web::testing` or `test-support`) that other tests can install.
- [ ] A test checks that a request's span carries its `operation_id`.
- [ ] A test checks that a request with a `traceparent` header is in the caller's trace.
- [ ] `.env.example` documents the `OTEL_*` variables and the defaults above.
- [ ] The "OpenTelemetry seam" note in `crates/AGENTS.md` is replaced by a short how-to: how to add a span (and its attributes) to backend code.
- [ ] `make check-changed` passes.
