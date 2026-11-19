---
id: 0050
name: "The worker reports queue, relay and backfill metrics read from Postgres every 15 seconds"
status: in-progress
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0049-request-side-metrics-are-exported-over-otlp-http-m.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---

# The worker reports queue, relay and backfill metrics read from Postgres every 15 seconds

Have the worker export the delivery metrics. Exactly one worker at a time reads them from Postgres, every 15 seconds; use an advisory lock or similar so that several workers don't report duplicates. The attempts histogram is recorded as jobs run.

## Acceptance criteria
- [ ] Exactly one worker at a time reads the Postgres-sourced figures, every 15 seconds.
- [ ] Per queue, the worker reports:
  - [ ] jobs due (`run_at <= now`, not dead);
  - [ ] jobs waiting for a later `run_at`;
  - [ ] the age of the oldest due job;
  - [ ] dead letters.
- [ ] Per queue, job attempts are recorded as a histogram when jobs run.
- [ ] Relay: the worker reports undispatched outbox rows and the age of the oldest one.
- [ ] Backfill: the worker reports events still to copy, per subscription.
- [ ] A failure to read the figures is logged and doesn't stop the worker.
- [ ] A test using the in-memory metric exporter checks that every one of these metrics is registered.
- [ ] A test checks that the figures reflect seeded queue state (e.g. a due job, a future job, a dead letter, an undispatched outbox row).
- [ ] `make check-changed` passes.
