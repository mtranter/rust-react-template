---
id: 0009
name: Make event delivery safe across rolling deploys, and ordered per entity
status: todo
priority: P1
size: M
blocked_by: []
blocks: []
source: discussion 2026-11-01 (event delivery latency, ordering and multi-instance safety)
---

# Make event delivery safe across rolling deploys, and ordered per entity

## Description
The outbox and worker (0007) are correct while every running process has the same
build. During a rolling deploy, old and new workers run against one database at
the same time, and two things go wrong without any crash:

1. **An old worker dead-letters a new subscription's jobs.** `run_next` claims any
   due job, whatever its queue. An old worker that claims a job for a queue added
   in the new build fails it with `NoHandler`, and the failed claim uses up one of
   its `MAX_ATTEMPTS` (5). With backoff of 10s to 160s, a rollout lasting a few
   minutes can dead-letter real work.
2. **An old relay drops a new subscription's deliveries for good.** `relay` fans
   out with its own process's `Handlers::subscribers`, then marks the event
   dispatched. If an old worker relays an event, no job is ever created for a
   subscription it doesn't know about, and the event is never offered again.
   The reverse happens when a subscription is removed: a new relay stops creating
   its jobs while old workers would still run them.

Separately, **consumers can't tell a newer state from an older one.** Delivery is
at least once and in no order, by design (CLAUDE.md "Side effects and reactions").
Several things reorder events:
- the outbox `seq` comes from a sequence at INSERT time, not at commit time;
- several workers claim jobs at once;
- a failed job backs off while later jobs run.

Events carry the aggregate's whole state (event-carried state transfer), but not
its version. So a subscriber that keeps a copy of that state can overwrite version
4 with a late-arriving version 3. The aggregate's optimistic-concurrency `Version`
already orders writes per aggregate, because writes are version-guarded UPDATEs,
so it is the natural ordering key. It just doesn't reach consumers.

A related doc bug: `EventEnvelope::time` says it is when the unit of work
committed. It is actually `clock.now()` at publish, before commit, on whichever
instance ran the transaction. It must not be used to order events.

## Evidence (as of 2026-11-01, `main` at `973f8a7`)
- `crates/common/db/src/worker.rs`, `run_next`: the claim's
  `WHERE dead_at IS NULL AND run_at <= $1` has no queue filter, and an unknown
  queue fails in `Handlers::handle` (`crates/common/persistence/src/messaging.rs`,
  `NoHandler`), after the claim has already counted as an attempt.
- `crates/common/db/src/worker.rs`, `relay`: it loops over
  `self.handlers.subscribers(&event.event_type)` and then sets `dispatched_at` for
  the whole batch.
- `crates/common/db/migrations/0001_messaging.sql`: `messaging.outbox.seq` is `bigserial`, and there
  is no subscription table.
- `crates/common/events/src/envelope.rs:21`: the `time` doc comment.
  `crates/common/db/src/publishing.rs:25`: `let now = self.clock.now();`.
- Event state types (`users_api::events::v1::User`, `notes_api::events::v1::Note`,
  the template's `v1::Widget`) have no version field.

## Proposed approach
Do this as three commits, in this order. Each one leaves `make check` green.

1. **Workers claim only the queues they serve.**
   - Add a way to list a `Handlers` registry's queue names in `app-persistence`
     (for example `Handlers::queues()`).
   - Filter the claim with `AND queue = ANY($n)`.
   - Update `job_due_idx` if the plan needs it (check with `EXPLAIN`).
   - A job whose queue no running worker serves now waits, rather than failing.
     Say so in the `NoHandler` doc comment and in spec §4. `NoHandler` itself
     stays as a guard.
2. **Store subscriptions in the database, and fan out in SQL.**
   - Add a `messaging.subscription (queue text PRIMARY KEY, event_type text NOT NULL, registered_at timestamptz NOT NULL)` table.
   - Every worker upserts its registry into it at startup (`Worker::new` or the
     start of `run`/`run_until_idle`, whichever keeps the test drivers
     deterministic).
   - The relay becomes `INSERT INTO messaging.job … SELECT … FROM outbox JOIN subscription USING (event_type)`.
   - Semantics to document in spec §4: a subscription receives every event
     relayed after any worker first registered it. Removing one is explicit: a
     migration, or a documented `DELETE`, never a side effect of deploying.
     Decide what an upsert does when the same queue name is registered for a
     different `event_type`. Failing the worker's startup loudly is the likely
     answer, since 3ad3e53 on `domain-events` already refuses mismatched
     deliveries.
3. **Carry the aggregate's version on every event.**
   - Prefer the CloudEvents extension attribute `sequence` on `EventEnvelope`,
     typed in Rust, so it's set once by the publisher rather than per state type.
     The other option is a field on each `-api` state type. Pick one and record
     why in spec §4.
   - The version has to come from `save`'s return value (or the deleted
     version), so the publish call needs it. Fit it to the `PendingEvent` API
     from the `domain-events` branch (see Notes).
   - Fix the `time` doc comment to say what it is.
   - Add the rule to CLAUDE.md: a subscriber that keeps a copy of state applies
     an event only if its version is newer than the one it has.

Out of scope: lowering `APP_WORKER_POLL_MS`, `LISTEN`/`NOTIFY`, relaying after
commit, and per-consumer cursors over the outbox. All were discussed and set aside.
- `NOTIFY` takes a database-wide lock at commit and doesn't work behind
  transaction-mode poolers.
- Cursors over the outbox need visibility checks based on transaction ids
  (`pg_snapshot_xmin`), because `seq` order isn't commit order.
- Tuning the poll interval is a separate one-line config change.

## Acceptance criteria
- [ ] A test with two `Worker`s on one database, built from different `Handlers`
      (the "old" one without a queue, the "new" one with it): the old worker
      never claims the new queue's job, and the new worker runs it. The test
      fails on `main`.
- [ ] A test where the old worker relays an event first: the new subscription
      still gets its job once the new worker has registered. The test fails on
      `main`.
- [ ] A test that a queue name registered for two different event types is
      refused.
- [ ] Every published event carries its aggregate's version.
- [ ] The `users`/`notes`/template drivers' `announced` assertions cover it,
      including the deleted version for `UserDeleted`.
- [ ] `notes`' `archive_notes_by` publishes each note's new version.
- [ ] `EventEnvelope::time` is documented accurately.
- [ ] Spec §4 and CLAUDE.md describe: claim filtering, durable subscriptions and
      their start-from-registration semantics, how to retire a subscription, and
      the "apply only if newer" rule for consumers.
- [ ] `make check` passes. `make prepare` has been run for the changed `query!`s.

## Open questions
- Should the version be the `sequence` envelope attribute or a field on the state
  type? Putting it in the envelope keeps state types free of persistence
  concerns, but every `EnvelopeOf<E>` consumer and test helper changes.
- When is the registry upserted? Worker start is simple. But a server-only
  deploy with no new worker yet would then publish events whose new subscribers
  aren't registered. That's acceptable: it's the same "from registration"
  semantics.
- Should removing a subscription also delete its pending jobs, or leave them for
  a human to look at?

## Notes
- **Testing.** Worker tests belong in `crates/common/db/tests/` (which today has
  only `map_db_error.rs`), as `#[sqlx::test(migrations = false)]`. Use two
  `Handlers` registries with small test events, as in the unit tests in
  `crates/common/persistence/src/messaging.rs` on the `domain-events` branch.
  `crates/server/tests/composed_reactions.rs` covers the composed app and must
  still pass.
- **Branches.** The `domain-events` branch (unmerged, 4 commits) replaces
  `Announcement` with `PendingEvent`, moves event selection into each domain
  (`UserEvent`, `NoteEvent`, `WidgetEvent`), and refuses deliveries whose `type`
  isn't the queue's. Commit 3 here touches the same publish path, so start from
  that branch if it has merged by then, or rebase onto it.
- Read CLAUDE.md "Side effects and reactions" and spec §4 first. Copy `notes`
  for driver changes.
