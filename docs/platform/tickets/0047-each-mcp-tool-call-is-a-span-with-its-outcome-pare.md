---
id: 0047
name: "Each MCP tool call is a span with its outcome, parent of its in-process REST call"
status: ready-for-agent
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: []
---

# Each MCP tool call is a span with its outcome, parent of its in-process REST call

Give every MCP tool call at `/mcp` its own span, named by the tool. The in-process REST call it makes (ADR 0034) runs as a child of that span.

## Acceptance criteria
- [ ] Each tool call produces one span named by the tool (its `operation_id`), with its outcome as an attribute (`isError`).
- [ ] The REST request span that the tool call makes in-process is a child of the tool call's span, in the same trace.
- [ ] A test using the in-memory exporter checks the tool span's name and outcome, and that the REST span is its child. Include one failing call (`isError: true`).
- [ ] `make check-changed` passes.
