---
id: 0053
name: "A Module defers a keyed job to its own handler in a Unit of Work, and a note archives itself at its archiveAt"
status: done
spec: docs/platform/specs/0004-modules-schedule-deferred-and-recurring-work-through-the-existing-queue.md
blocked_by: []
blocks: [docs/platform/tickets/0054-a-module-declares-a-cron-schedule-that-the-worker.md]
---

# A Module defers a keyed job to its own handler in a Unit of Work, and a note archives itself at its archiveAt

First slice of Spec 0004: **Deferred Jobs** on the existing `messaging.job` queue, with `notes`' optional `archiveAt` as the reference user. Also write the new ADR that supersedes ADR 0015. It covers both deferred and recurring work as the Spec decides, so the schedule Tickets only build on it.

A deferred job is a command to the Module itself, not an Event. It has its own type, with a stable name and a serde payload, and a handler registered in `Handlers` the way a subscriber's is, under a stable name. A `Tx`-scoped method (in `app_persistence`/`app_db`, beside the publisher) writes it, due at a given time and named by a key within its type. Deferring again with the same key replaces the pending job's `run_at` and payload. Cancelling by key removes the pending job, and is a no-op when the job has already run or never existed. The worker runs it as `Principal::System`, with the same lease, retries, backoff and dead letters as event jobs. It is due by the injected `Clock` (ADR 0020).

In `crates/reference/notes`, a note gets an optional `archiveAt` on create and update. Setting it defers the note's archive, keyed by the note id. Changing it reschedules the job, and clearing it cancels the job. When the job runs, it archives the note through the domain's `archive(now)`, saves the note and publishes `Archived`. If the note is already archived or gone, the job does nothing.

## Acceptance criteria
- [ ] A new ADR in `docs/platform/adr/` (next free number) records deferred and recurring work on `messaging.job` (the `messaging.schedule` registry, the tick in the worker's poll loop, `worker --drain`, cron crate behind its own type, UTC only). It marks ADR 0015 superseded, and ADR 0015 points to it.
- [ ] A Module defers a job from a use case with a `Tx` method taking the job type, key, due time and payload. It registers the job's handler in its `handlers(..)`. Both are shown in `crates/reference/notes`.
- [ ] The deferred job is written in the caller's `Tx`. A test shows that a change which rolls back leaves no job.
- [ ] A test shows that deferring with an existing pending key replaces that job's due time and payload rather than adding a second job.
- [ ] A test shows that cancelling by key removes the pending job, and that cancelling a key that already ran or never existed returns `Ok`.
- [ ] Deferred jobs run as `Principal::System`, only once due by the test clock (`later(by)`). They retry and dead-letter like event jobs, and a test covers a failing deferred handler reaching a dead letter.
- [ ] `notes` accepts, returns and persists an optional `archiveAt` (with a migration and seed defaults). Notes tests through the driver cover: archived at `archiveAt`; rescheduled when `archiveAt` changes; not archived when it is cleared.
- [ ] `make generate` has been run, and `frontend/openapi.json` and `frontend/src/api/` are committed. The frontend type-checks.
- [ ] `crates/AGENTS.md` replaces the "no jobs or scheduled work" rule and the "not built yet" entry with how to defer, reschedule and cancel work, pointing at `notes`. `docs/platform/glossary.md` (the platform's glossary, where the Spec's `GLOSSARY.md` terms belong) defines **Deferred Job**.
- [ ] `make check-changed` passes.
