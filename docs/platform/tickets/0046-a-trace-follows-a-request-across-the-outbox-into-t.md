---
id: 0046
name: A trace follows a request across the outbox into the handlers that react to it
status: in-progress
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---

# A trace follows a request across the outbox into the handlers that react to it

Carry W3C trace context in each event's envelope, using the CloudEvents distributed-tracing extension. The worker's handler then continues the trace of the request that published the event. Also give the worker spans for its own work: relay batches, backfill chunks, compaction runs and job runs.

## Acceptance criteria
- [ ] `EventEnvelope` has optional `traceparent` and `tracestate` extension attributes, serialized under those names and omitted when absent.
- [ ] Envelopes already in the outbox without these attributes still deserialize and are delivered.
- [ ] Publishing an event records the current trace context in its envelope. Events published outside any trace (e.g. by `System` in seeds) carry none.
- [ ] Each job run has a span with queue, event type, key, version and attempt.
- [ ] A job run's span is a child of the publishing span when the envelope carries a trace context, and starts a new trace when it doesn't.
- [ ] Each retry of a job is a sibling span in the same trace.
- [ ] The worker has a span for each relay batch, each backfill chunk and each compaction run.
- [ ] A test using the in-memory exporter checks that an event published in a request is handled in the same trace.
- [ ] A test checks that an envelope with no trace context is handled in a new trace.
- [ ] Existing envelope serialization tests are updated for the new attributes.
- [ ] `make check-changed` passes.
