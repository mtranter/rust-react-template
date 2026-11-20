---
id: 0052
name: "ADR: OTLP and the standard variables, trace context in the envelope, and the metric set"
status: done
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0045-errorbody-carries-the-trace-s-id-as-traceid.md, docs/platform/tickets/0046-a-trace-follows-a-request-across-the-outbox-into-t.md, docs/platform/tickets/0047-each-mcp-tool-call-is-a-span-with-its-outcome-pare.md, docs/platform/tickets/0048-each-sql-query-is-a-span-with-its-statement-but-ne.md, docs/platform/tickets/0049-request-side-metrics-are-exported-over-otlp-http-m.md, docs/platform/tickets/0050-the-worker-reports-queue-relay-and-backfill-metric.md, docs/platform/tickets/0051-an-optional-observability-compose-profile-runs-a-l.md]
blocks: []
---

# ADR: OTLP and the standard variables, trace context in the envelope, and the metric set

Record the decision as a platform ADR, numbered next in `docs/platform/adr/`. Make sure the docs match what landed.

## Acceptance criteria
- [ ] A new ADR in `docs/platform/adr/` records:
  - [ ] export over OTLP, configured only by the standard `OTEL_*` variables, off when no endpoint is set;
  - [ ] the parent-based default sampler;
  - [ ] the W3C trace context carried in the event envelope as the CloudEvents `traceparent`/`tracestate` extension, and how handlers continue the trace;
  - [ ] the fixed metric set and its names;
  - [ ] the options weighed and the consequences, including SQL span volume and Lambda flushing.
- [ ] The ADRs this one touches (e.g. 0011/0012 on the envelope, 0022 on `ErrorBody`, 0026 on pinned pairs) are cross-referenced where relevant.
- [ ] `crates/AGENTS.md` cites the new ADR in its span and metric how-to, and no longer lists an OpenTelemetry exporter as not built.
- [ ] `docs/platform/glossary.md` gains any terms the change introduced (e.g. trace context), if they're not already there.
- [ ] `make check-changed` passes.
