# Modules defer and schedule work on the job queue

Supersedes [ADR 0015](0015-no-jobs-or-scheduled-work.md) (Spec 0004).

ADR 0015 let a module act only in reaction to an event and built no scheduled work, since nothing needed it. Business workflows do: "remind in 3 days" (deferred), "close stale quotes nightly" (recurring), "this approval expires in 48 hours unless someone acts" (a deferred job that can be cancelled). Without a mechanism each module would invent its own. `messaging.job` already has a due time (`run_at`), a lease, retries with backoff, dead letters and per-key ordering, and a worker that polls it by the injected clock (ADR 0020), so both kinds of work become rows there. Postgres stays the one source of truth for what is due, and no state lives outside it.

**A deferred job is a command to the module itself, not an event.** ADR 0015's lesson stands: a command isn't dressed as an event (`UserRegistered` replaced `SendWelcomeEmail`). So a deferred job has its own type, implementing `app_persistence::DeferredJob`: a stable `NAME` (`notes.archive_note`), which is its queue in `messaging.job`, and a serde payload (the type itself), which only gains fields, as an event's entity does. Its handler implements `Run<J>` and is registered in the module's `handlers(..)` with `Handlers::runs::<J>(handler)`, beside its subscribers. Nothing else can subscribe to it, and `migrate` registers nothing for it: the type is the queue.

**It is written in the unit of work that causes it.** A service holds `Arc<dyn DeferredJobs>` (`Context::deferred_jobs()`, `app_db::PgDeferredJobs`) beside its publisher and calls `defer::<J>(&mut tx, key, at, &job)` or `cancel::<J>(&mut tx, key)` with the `Tx` it saved in, so the job exists if and only if the change commits.

**It is named by a key within its type.** At most one job of a type and key is pending (`job_deferred_key_idx`, a partial unique index over the rows marked `deferred` and not dead). Deferring again with the same key replaces the pending job's `run_at` and payload and starts its attempts afresh: a reschedule. Cancelling removes the pending job, and is `Ok` when it already ran or never existed. A dead letter steps aside: the next deferral of its key is a new job. A replacement takes the next `version` of the row, and the worker completes or fails a run only while the row still has the version it claimed, so a run under way when its job is rescheduled or cancelled takes no effect (its writes roll back) and the new job runs at its own time.

**It runs as any delivery does.** The worker claims deferred jobs beside subscriptions' (`Handlers::claims`), once their `run_at` has passed by its clock, under the same lease, retries, backoff and dead letters (`MAX_ATTEMPTS`). Its handler runs inside the transaction that removes the job, acting as `Principal::System`, as reactions do, and must be idempotent: at least once, as ever. A deferred job carries no trace context: it is a root of its own trace when it runs.

**Recurring work is a schedule, declared in code** (built by the next ticket of Spec 0004; decided here so it builds on this):

- A schedule has a name, a cron expression of five fields, always UTC (no time zones or daylight saving), and a handler. The cron crate sits behind a type of the platform's own, so the workspace's time crate (ticket 0030 plans to move off `chrono`) can change without touching modules.
- `migrate` registers each in a new `messaging.schedule` table, as it registers topics and subscriptions: a changed expression is updated and its next due time recomputed; one removed from code is retired and its pending run removed, serialised against the worker as retiring a subscription is.
- **The tick is the worker's existing poll loop.** Each pass of `run_until_idle` turns due slots into jobs, beside relaying and backfilling, with no separate timer or task. Several workers never create a slot twice; missed slots after downtime run once, not once per slot; a schedule never has two runs pending or running at once (a slot due while one runs is skipped and logged). Handlers run as `System` and are told their slot.
- **`worker --drain [--max-seconds N]`** relays, backfills, schedules and runs until nothing is due by its clock, or its budget is spent, then exits `0`; failures are retried and dead-lettered as usual, and it exits non-zero only when it can't work at all (the database unreachable, its clock refused). So any external timer (cron, a container scheduler, later a cloud scheduler) can drive the worker. `worker` without `--drain` is unchanged.

## Considered options

- **Keep ADR 0015: model the time as an event.** A module would publish an event and a subscriber would wait for its time, but a subscriber can't wait without holding up its key, and "archive this at 3pm" is a command, not a fact; it would be ADR 0015's mistake again.
- **A separate scheduler (a job library, a cron process, a cloud scheduler) as the source of truth.** One more thing to deploy and keep consistent with the database: a job scheduled outside the unit of work could exist for a change that rolled back. A cloud timer may still *drive* `worker --drain`; it never holds what is due.
- **A table of its own for deferred jobs.** It would duplicate the lease, retries, backoff, dead letters and the claim the job queue already has. The queue gained a `deferred` flag and an index instead.
- **Several pending jobs per key, or keys across types.** Rescheduling would then mean cancelling and adding, and two types could clobber each other's keys. One per type and key makes reschedule an upsert.
- **Event-style ordering by key for deferred jobs.** There is only ever one pending per key, so the version is used only to tell a replaced job from the run that claimed it.

## Consequences

- A module may now do work at a time it chooses, deferred and cancellable by key (`crates/reference/notes`' `archiveAt`), and, once built, on a schedule.
- A deferred job's type is a queue name forever: renaming it strands its pending jobs, and removing its handler leaves them waiting for a worker that can run them. Retire one by dead-lettering its pending jobs in a migration (`UPDATE messaging.job SET dead_at = now(), last_error = 'retired' WHERE queue = '<name>' AND dead_at IS NULL`).
- Rerunning a deferred dead letter while a later job of its key is pending fails on the unique index: cancel or let the pending one run first.
- Deferred jobs are seen, retried and dead-lettered with SQL on `messaging.job`, like event jobs, until an operator surface exists (Spec 0005).
- Compaction never touches `messaging.job`, so deferred jobs are unaffected by it; the worker's compaction stays its own maintenance, not module work.
