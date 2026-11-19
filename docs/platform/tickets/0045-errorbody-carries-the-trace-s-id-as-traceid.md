---
id: 0045
name: "ErrorBody carries the trace's id as traceId"
status: ready-for-agent
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---

# ErrorBody carries the trace's id as traceId

Add an optional `traceId` to `ErrorBody`, next to `requestId`. It is filled from the current span whenever tracing is on, so a failed REST call or a failed MCP tool result points straight at its trace.

## Acceptance criteria
- [ ] `ErrorBody` has an optional `traceId` (camelCase in the spec), documented as the W3C trace id of the request.
- [ ] `traceId` is present on every rendered error, both handler errors and `render_error` refusals, whenever a trace is active.
- [ ] `traceId` is omitted when there is no trace.
- [ ] The MCP server's instructions mention `traceId`, beside `requestId`, as the way to look up a failed call.
- [ ] `make generate` is run, and the updated `frontend/openapi.json` and `frontend/src/api/` are committed. `make check-spec` passes.
- [ ] A test using the in-memory exporter checks that an `ErrorBody` carries the id of the request's trace.
- [ ] `make check-changed` passes.
