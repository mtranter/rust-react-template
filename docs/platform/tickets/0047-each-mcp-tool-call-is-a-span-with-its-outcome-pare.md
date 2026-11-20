---
id: 0047
name: "Each MCP tool call is a span with its outcome, parent of its in-process REST call"
status: done
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---
# Each MCP tool call is a span with its outcome, parent of its in-process REST call

Give every MCP tool call at `/mcp` its own span, named by the tool. The in-process REST call it makes (ADR 0034) runs as a child of that span.

## Acceptance criteria

- [ ] Each tool call produces one span named by the tool (its `operation_id`), with its outcome as an attribute (`isError`).
- [ ] The REST request span that the tool call makes in-process is a child of the tool call's span, in the same trace.
- [ ] A test using the in-memory exporter checks the tool span's name and outcome, and that the REST span is its child. Include one failing call (`isError: true`).
- [ ] `make check-changed` passes.

## Needs Human Input (factory)

The factory stopped on this Ticket: implementor did not complete the Ticket: no structured output (session 1fa8f01c-ddfe-4a84-b381-98205cde9a76)

### From Human 
Try again

## Needs Human Input (factory)

The factory stopped on this Ticket: factory/0047-each-mcp-tool-call-is-a-span-with-its-outcome-pare is not on top of main after the rebase (session 8d8753da-4c43-4e86-bdc5-af0ba4502a06)
