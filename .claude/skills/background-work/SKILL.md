---
name: background-work
description: Make a module do work at a later time (a deferred job: defer, reschedule, cancel) or on a recurring cron (a Schedule), retire either, or test them. Use before touching `DeferredJob`, `Run<J>`, `Handlers::runs`, `Schedule`, `RunScheduled`, `Handlers::every`, `ctx.deferred_jobs()` or `messaging.schedule`.
---

# Deferred jobs and Schedules (ADR 0043)

Extends `crates/AGENTS.md`; if this file and the code disagree, the code wins: fix this file in the same change. Both run in the worker with the lease, retries and dead letters of event jobs (see the `events-and-projections` skill), act as `Principal::System`, and must be idempotent.

| Doing | Copy |
|---|---|
| Work at a time: deferring, rescheduling or cancelling | `crates/reference/notes`' `application/jobs.rs` (`ArchiveNote`), its service's `schedule_archive(..)` and `archive_when_due(..)`, its `handlers(..)` (`runs::<ArchiveNote>`), and its tests' `behaviours/archiving_at.rs` |
| Recurring work | `crates/reference/notes`' `application/schedules.rs` (`PurgeArchivedNotes`), its service's `purge_archived(..)`, its `handlers(..)` (`every::<PurgeArchivedNotes>`), and its tests' `behaviours/purging.rs` |

## A deferred job: work at a time

A command to the module itself, never an event.

1. Declare its type in `application/`: `#[derive(Serialize, Deserialize)]`, `impl app_persistence::DeferredJob` with a stable `NAME`, `<module>.<what it does>`, which is its queue forever. Its payload only gains `#[serde(default)]` fields.
2. Implement `Run<J>` for its handler: acting as `Principal::System`, idempotent, doing nothing when the state no longer calls for it.
3. Register it in `handlers(..)` with `Handlers::runs::<J>(service)`. `migrate` registers nothing for it.
4. The service holds `ctx.deferred_jobs()` (`Arc<dyn DeferredJobs>`) and, in the `Tx` it saves in, after the save, calls `self.deferred.defer(&mut tx, key, at, &job)` (deferring again with the same key reschedules: it replaces the pending job's time and payload) or `self.deferred.cancel::<J>(&mut tx, key)` (`Ok` when it already ran or never existed). Key it by the aggregate it acts on.

It runs once due by the worker's clock. A run under way when its job is rescheduled or cancelled rolls back. Removing a job type is a migration dead-lettering its pending jobs.

## A Schedule: recurring work

Declared in code, never edited at runtime.

1. Declare a unit type in `application/` implementing `app_persistence::Schedule`: a stable `NAME`, `<module>.<what it does>` (its row in `messaging.schedule` and its runs' queue, forever), and `CRON`, five fields, always UTC (`0 3 * * *`; numbers, `*`, ranges, steps and lists, no names or `@daily`: `app_db::Cron`).
2. Implement `RunScheduled<S>` for its handler: acting as `Principal::System`, idempotent, deciding by the `slot` it is given, never the clock. After downtime it runs once, for the last slot missed.
3. Register it in `handlers(..)` with `Handlers::every::<S>(service)`. `migrate` registers it, due at its next slot; changing `CRON` reschedules it from the next `migrate`, and `migrate` and the worker refuse to start on one that doesn't parse, naming it.

Each pass of the worker's poll loop (`Worker::schedule`, in `run_until_idle`) turns each due slot into one job, whatever the number of workers. A slot due while the last run is still pending or running is skipped, logged. A run that saves several roots saves and announces each as its own change, in its one `Tx`.

**Retire a Schedule by deleting its declaration**: the next `migrate` marks every registered Schedule its build doesn't declare retired (`retired_at` in `messaging.schedule`; no migration to write), so no worker makes it another run, deletes its run not yet claimed, and dead-letters one a worker has claimed (`schedule retired`): that run finishes, its success removing it, or stays dead, and never runs twice. It is serialised against the workers' passes, so none adds a run after it. Declaring a retired Schedule's name again reactivates it, due at its next slot after that `migrate`, unlike a retired queue's name.

## Testing

Tests move time with `later(by)`, then `run_pending_events()`. A Schedule's tests register it with `app_db::register_schedules` in the driver's boot (not the `TEMPLATE`, since boot is per test), then move time past a slot.
