---
id: 0013
name: A projection has caught up only once every event its topic held at registration is handled, judged by outbox `seq` rather than worker clocks
status: done
priority: P1
size: M
blocked_by: []
blocks: [0019]
source: PR 3 review (Copilot, "Use a durable backfill boundary instead of worker timestamps" and "Wait for all pre-registration events before reporting readiness"), confirmed by repro 2026-11-15
---

# A projection has caught up only once every event its topic held at registration is handled, judged by outbox `seq` rather than worker clocks

## Description

`app_db::caught_up` promises that a copy "holds every key its topics held when `migrate` registered them" (ADR 0036). It reports that too early in two ways, and `Generations` then latches onto the incomplete copy and serves it instead of answering 503.

1. **It compares clocks of different workers.** A job counts towards readiness when `job.created_at <= backfill.completed_at`. `created_at` is the clock of the worker that copied the chunk; `completed_at` is the clock of the worker that later found nothing left to copy. If the second worker's clock is a few milliseconds behind the first's, the last chunk's jobs fall after `completed_at` and are not waited for.
2. **It ignores events that were published but not yet relayed at registration.** The backfill covers only events already relayed (`relay_batch <= through_batch`). Events committed to the outbox but still undispatched when `migrate` registers the subscription are delivered by the relay, 100 per batch (`RELAY_BATCH`). The empty backfill completes after the first batch, so jobs of the second and later batches are created after `completed_at` and are not waited for. This is the `make dev` path (`migrate`, then `seed`, then the worker) for any seed set over 100 events, and any deployment whose relay is behind when `migrate` runs.

Both come from using wall-clock time as the boundary. The fix replaces it with a boundary in outbox `seq`, recorded durably at registration.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/projection.rs` `caught_up`: `j.created_at <= b.completed_at`.
- `crates/common/db/src/worker.rs` `copy_chunk`: both `created_at` (the job insert) and `completed_at` are `self.clock.now()` of whichever worker runs that chunk.
- `crates/common/db/src/subscriptions.rs` `backfill`: records only `through_batch`; nothing records the undispatched events present at registration.

Both were reproduced with these tests appended to `crates/common/db/tests/delivery/projection.rs`. Each **fails today** with the message shown; each must pass once this ticket is done (rename them to state the behaviour, per the test naming rule in `crates/AGENTS.md`).

```rust
// Fails today: "caught up with 100 of 250 keys"
#[app_test_db::test(crate::support::TEMPLATE)]
async fn repro_undispatched_events_at_registration(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    let events: Vec<_> = (0..250).map(|i| ping(&format!("key{i}"), 1)).collect();
    messaging.publish(events).await.unwrap();
    let handlers = projecting(COPY);
    messaging.register(&handlers).await.unwrap();
    let worker = messaging.worker_of(handlers);

    // One pass of `run_until_idle`'s loop.
    assert_eq!(worker.relay().await.unwrap(), 100);
    worker.backfill().await.unwrap();
    while worker.run_next().await.unwrap() {}

    let held = rows(&messaging).await.len();
    let ready = caught_up(&messaging.pool, &[COPY]).await.unwrap();
    assert!(!ready, "caught up with {held} of 250 keys");
}

// Fails today: "caught up holding 1 of 2 keys, a job unhandled"
#[app_test_db::test(crate::support::TEMPLATE)]
async fn repro_completing_worker_clock_behind_copying_worker(pool: PgPool) {
    let messaging = with_copy_table(pool).await;
    messaging.migrate(&["tests.first"]).await;
    messaging
        .publish(vec![ping("alice", 1), ping("bob", 1)])
        .await
        .unwrap();
    run(&messaging.worker(&["tests.first"])).await;
    messaging.register(&projecting(COPY)).await.unwrap();
    let worker = messaging.worker_of(projecting(COPY)).backfilling_in_chunks_of(1);

    assert_eq!(worker.backfill().await.unwrap(), 1);
    assert!(worker.run_next().await.unwrap());
    // Worker A copies the last chunk; worker B, 5ms behind, finds none left.
    messaging.later(TimeDelta::milliseconds(10));
    assert_eq!(worker.backfill().await.unwrap(), 1);
    messaging.later(TimeDelta::milliseconds(-5));
    assert_eq!(worker.backfill().await.unwrap(), 0);

    let held = rows(&messaging).await.len();
    let ready = caught_up(&messaging.pool, &[COPY]).await.unwrap();
    assert!(!ready, "caught up holding {held} of 2 keys, a job unhandled");
}
```

Note for the second repro: `PingTopic` has several event types, so a whole-topic projection has one `messaging.backfill` row per type. An empty type's row completes in the first `backfill()` call, which is why the test needs two chunks and one handled job before the skew shows.

## Proposed approach

Every event the topic held at registration has an outbox `seq` at or below the outbox's highest `seq` at that moment. Record that `seq` on the backfill, record on every job the `seq` of the event it delivers, and define readiness in those terms. No timestamp is compared.

### 1. Migration (new file, next free number in `crates/common/db/migrations/`)

Do not edit `0005_incremental_backfill.sql` or `0006_projection_version.sql`: dev databases have applied them and sqlx checksums them.

```sql
-- The outbox event each job delivers, so readiness can be judged by `seq`
-- rather than by clocks (ticket 0013). Jobs from before this column count as 0,
-- at or below every boundary, so they are waited for.
ALTER TABLE messaging.job ADD COLUMN outbox_seq bigint NOT NULL DEFAULT 0;
ALTER TABLE messaging.job ALTER COLUMN outbox_seq DROP DEFAULT;

-- The outbox's highest `seq` when `migrate` registered the subscription: every
-- event its topic then held, relayed or not, is at or below it.
ALTER TABLE messaging.backfill ADD COLUMN through_seq bigint NOT NULL DEFAULT 0;
UPDATE messaging.backfill
SET through_seq = (SELECT coalesce(max(seq), 0) FROM messaging.outbox);
ALTER TABLE messaging.backfill ALTER COLUMN through_seq DROP DEFAULT;

CREATE INDEX job_outbox_seq_idx ON messaging.job (queue, event_type, outbox_seq)
    WHERE dead_at IS NULL;
```

The `UPDATE` makes backfills registered before this migration conservative (they wait for everything now in the outbox). Dropping the defaults makes every insert state its `seq`, so a forgotten one fails rather than silently counting as 0. Column names are suggestions; keep them distinct from the existing `after_seq` (the copy cursor) and `through_batch` (the relayed-batch boundary).

### 2. Registration: `crates/common/db/src/subscriptions.rs` `backfill`

Add `through_seq` to the `INSERT INTO messaging.backfill`, as `(SELECT coalesce(max(seq), 0) FROM messaging.outbox)`, in the same statement that reads `through_batch`.

Why this is the right boundary:
- An event committed before this statement is visible to it, so its `seq` is at or below the maximum. It is either already relayed (in the backfill) or undispatched (the relay will deliver it): both are waited for.
- A transaction still in flight that already took a lower `seq` commits later, is delivered by the relay, and is also waited for. That is more than promised and harmless.
- An event with a higher `seq` is published after registration: live, not part of readiness.

### 3. Both job inserts in `crates/common/db/src/worker.rs` set `outbox_seq`

- `relay`: the insert selects from `messaging.delivery`, which already exposes `seq`.
- `copy_chunk`: the `chunk` CTE already selects `seq`.

Search the repo for any other `INSERT INTO messaging.job` (tests, scripts) and set it there too; as of this ticket only those two exist.

### 4. `caught_up` in `crates/common/db/src/projection.rs`

Keep the subscription check as it is (every queue registered and not retired). Replace the backfill condition so a queue is *not* caught up while any of its backfill rows `b` has:

- `b.completed_at IS NULL` (not all copied), or
- an outbox event with `dispatched_at IS NULL AND topic = b.topic AND event_type = b.event_type AND seq <= b.through_seq` (held at registration, not yet relayed), or
- a job with `queue = b.queue AND event_type = b.event_type AND dead_at IS NULL AND outbox_seq <= b.through_seq` (relayed or copied, not yet handled).

A dead-lettered job still does not hold readiness back; that is deliberate (ticket 0017 makes it loud). `completed_at` and `created_at` stay as columns for visibility, but no query compares them any more.

The undispatched lookup is served by `outbox_undispatched_idx` (few rows in steady state); the job lookup by the new index.

### 5. Docs, in the same change

- The doc comment on `caught_up`.
- ADR 0036, the paragraph "A copy is served only once it has caught up" (it says "every job copied by then is handled"): state the `seq` boundary and that undispatched events at registration count.
- ADR 0035 where it describes what `migrate` records.
- `crates/AGENTS.md`, the `earliest` paragraph ("`app_db::caught_up`: backfill copied and handled"), and `GLOSSARY.md` if it defines caught up.
- `make prepare` to refresh `.sqlx` (three queries change).

## Acceptance criteria

- [x] Both repro tests above are in `tests/delivery/projection.rs` under behaviour names and pass.
- [x] A test shows a live event published *after* registration does not hold readiness back: with the backfill handled and one later event still unrelayed, `caught_up` is true.
- [x] `caught_up` contains no comparison of timestamps.
- [x] Every existing test in `crates/common/db/tests/delivery/` passes unchanged, apart from helpers that insert or compare job rows needing the new column.
- [x] ADR 0035, ADR 0036, `crates/AGENTS.md` and the `caught_up` doc comment describe the new rule.
- [x] `.sqlx` regenerated; `make check` passes.

## Resolution

Done as proposed. Migration `0007_readiness_by_outbox_seq.sql` adds `messaging.job.outbox_seq` and `messaging.backfill.through_seq` (backfilled conservatively, defaults dropped) and `job_outbox_seq_idx`. `register_subscriptions` records `through_seq` in the statement that reads `through_batch`; the relay and `copy_chunk` set each job's `outbox_seq`. `caught_up` now waits on any backfill row that is not copied, has an undispatched event of its topic and type at or below `through_seq`, or has a live job of its queue and type at or below `through_seq`; it compares no timestamps. Tests in `tests/delivery/projection.rs`: `a_projection_waits_for_the_events_still_unrelayed_when_it_was_registered`, `a_projection_waits_for_its_last_copied_chunk_whatever_the_workers_clocks` (the two repros), and `an_event_published_after_registration_does_not_hold_a_projection_back` (unrelayed) and `an_event_relayed_after_registration_does_not_hold_a_projection_back` (relayed, job unhandled), one for each side of the `through_seq` bound. Four queries changed in `.sqlx`, not three: the relay's job insert too.

## Notes

- The per-key ordering guard in `Worker::run_next` is correct as it is and is not part of this ticket.
- Ticket 0019 builds on `through_seq` and should follow this one.
