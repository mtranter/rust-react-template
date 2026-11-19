---
id: 0048
name: Each SQL query is a span with its statement but never its bound values
status: in-progress
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---

# Each SQL query is a span with its statement but never its bound values

Trace database queries. Each query run through the app's sqlx pool gets a span carrying the statement text and none of its bound values.

## Acceptance criteria
- [ ] Every query made through the app's pool, by request handlers and by the worker, produces a span that is a child of the current span.
- [ ] Each query span carries the statement text (`db.query.text`, following the semantic conventions) and the database system.
- [ ] Bound values never appear in the span.
- [ ] A test using the in-memory exporter checks that a request that queries the database has child SQL spans with statement text, and that a bound value it sent is absent from every span attribute.
- [ ] `make check-changed` passes.
