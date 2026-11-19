---
id: 0049
name: "Request-side metrics are exported over OTLP: HTTP, MCP, database pool and idempotency"
status: ready-for-agent
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: []
---

# Request-side metrics are exported over OTLP: HTTP, MCP, database pool and idempotency

Add an OTLP meter provider, set up alongside the tracer from the same `OTEL_*` variables, and flushed on shutdown. Record the metrics measured where requests are served. Follow OpenTelemetry's semantic conventions where one exists.

## Acceptance criteria
- [ ] Metrics are exported over OTLP when `OTEL_EXPORTER_OTLP_ENDPOINT` is set, and nothing is exported when it isn't.
- [ ] Exporting metrics never blocks a request.
- [ ] Every binary flushes its meter provider on shutdown.
- [ ] HTTP: request duration is a histogram (semconv `http.server.request.duration`), by `operation_id`, method and status.
- [ ] MCP: tool calls are counted by tool and outcome.
- [ ] Database pool: connections in use, idle, and maximum, and how long acquiring a connection waits (a histogram).
- [ ] Idempotency: a counter of replays served and a counter of keys refused for reuse.
- [ ] A test using an in-memory metric exporter checks that each of these metrics is registered, after exercising the request path where needed.
- [ ] `crates/AGENTS.md` gains a short how-to for adding a metric, next to the span how-to.
- [ ] `make check-changed` passes.
