---
id: 0054
name: "A Module declares a cron Schedule that the worker's poll loop runs once per due slot, and notes purges old archived notes nightly"
status: done
spec: docs/platform/specs/0004-modules-schedule-deferred-and-recurring-work-through-the-existing-queue.md
blocked_by: [docs/platform/tickets/0053-a-module-defers-a-keyed-job-to-its-own-handler-in.md]
blocks: [docs/platform/tickets/0055-a-schedule-removed-from-code-is-retired-by-migrate.md]
---

# A Module declares a cron Schedule that the worker's poll loop runs once per due slot, and notes purges old archived notes nightly

Recurring work for Spec 0004. A Module declares Schedules in code, each with a name, a five-field UTC cron expression and a handler, beside its topics and subscriptions. `migrate` registers them in a new `messaging.schedule` table. Each pass of the worker's existing loop (`run_until_idle` in `crates/common/db/src/worker.rs`) turns due slots into jobs, alongside relaying and backfilling. There is no separate timer. The jobs reuse the own-handler job path built for Deferred Jobs.

On the cron crate (an open risk in the Spec): ticket 0030 plans to move off `chrono`. Keep the crate behind a small type of `app_db`'s own (parse, then next slot after an instant), so swapping the time crate later touches one file. Hand-rolling five-field UTC parsing is acceptable if no suitable crate fits. Record the choice in the ADR from t1.

Reference: `notes` declares a nightly Schedule that purges notes archived more than 30 days ago (by the injected clock), with each purge saved and announced as its own change.

## Acceptance criteria
- [ ] A Module declares a Schedule (name, cron expression, handler) in its `module(..)`/`handlers(..)`. An invalid expression makes `migrate` (and the worker) refuse to start, naming the Schedule.
- [ ] `migrate` inserts new Schedules into `messaging.schedule` with their next due time. When a Schedule's expression changes, `migrate` updates it and recomputes its next due time. A test covers both.
- [ ] Each worker pass turns a due slot into exactly one job. A test with two workers on one database shows that a slot creates exactly one job.
- [ ] After downtime spanning several slots, the Schedule runs once, not once per missed slot, and its next due time moves past now (test, driven by the test clock).
- [ ] A Schedule never has two runs pending or running at once. A slot that falls due during a run is skipped and logged (test).
- [ ] Schedule handlers run as `Principal::System`, are told the slot they are running for, and retry and dead-letter like other jobs.
- [ ] `notes` purges notes archived more than 30 days before the slot, nightly. A notes driver test shows that a note archived 31 days ago is purged and one archived 29 days ago is not.
- [ ] `crates/AGENTS.md` says how to declare a Schedule, pointing at `notes`. `docs/platform/glossary.md` defines **Schedule**.
- [ ] `make check-changed` passes.

## Needs Human Input (factory)

The factory stopped on this Ticket: implementor did not complete the Ticket: no structured output (session 00b86bcb-7f11-48c9-8a78-444604679b13)

### From Human 
Try again
