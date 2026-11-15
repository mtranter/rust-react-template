---
id: 0021
name: A handler that fails after its job was dead-lettered leaves it dead
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: PR 3 review of ticket 0015, 2026-11-15
---

# A handler that fails after its job was dead-lettered leaves it dead

## Description

`Worker::fail` records a failed attempt with an unconditional `UPDATE … SET dead_at = $4 WHERE id = $1`, where `$4` is NULL while attempts remain. If the job was dead-lettered while its handler ran, that update makes it live again:

1. A worker (say, of an older build that still has the handler) claims a job in queue X.
2. `messaging.retire('X')` commits: the subscription is retired and the job gets `dead_at` and `last_error = 'subscription retired'`.
3. The handler returns an error. `fail` sets `dead_at = NULL`, overwrites `last_error` and schedules a retry.

The job is now live in a retired queue: the old build runs it again, and on newer builds it sits in `messaging.job` forever. The same happens to a job dead-lettered any other way while its handler runs (by hand, or by ticket 0016's dead-lettering of a job whose worker died, when the earlier run turns out to be merely slow). `messaging.retire` (ticket 0015) is serialised against the relay and backfills but can't cover this, since the claim has already committed.

## Evidence (as of 2026-11-15)

`crates/common/db/src/worker.rs` `fail`:

```rust
sqlx::query!(
    r#"
    UPDATE messaging.job SET last_error = $2, run_at = $3, dead_at = $4
    WHERE id = $1
    "#,
    job.id,
    error,
    now + backoff(job.attempts),
    dead.then_some(now),
)
```

The claim's transaction has committed before the handler runs, and the handler's transaction (`complete`) doesn't touch the job row until its `DELETE`, so nothing stops another transaction dead-lettering the job meanwhile.

## Proposed approach

In `fail`, add `AND dead_at IS NULL` to the `UPDATE` and read `rows_affected()`:

- 1 row: as today. Log "job failed; will retry" or "job dead-lettered" after the update rather than before it, so neither is logged for a job it didn't change.
- 0 rows: the job was dead-lettered (or deleted) while its handler ran. Leave it as it is, its `last_error` included (`subscription retired` says why better than the handler's error), log at warn with the queue and the handler's error ("job failed after it was dead-lettered; left dead"), and return `Ok(())`.

Do **not** make `complete`'s `DELETE` conditional on `dead_at`: a job dead-lettered while its slow handler ran is rightly removed when that handler succeeds (tickets 0016 and 0018 rely on this).

## Acceptance criteria

- [x] A test in `crates/common/db/tests/delivery/`: a handler that, while running, has its subscription retired with `messaging.retire` from another connection, then returns an error, leaves its job dead with `last_error = 'subscription retired'`, and no later run (after `later(..)` past the backoff) claims it again. A test-local `Handle` holding a `PgPool` can retire inside `handle`.
- [x] A handler that fails with attempts left still schedules a retry, and one on its last attempt is still dead-lettered; the existing retry and dead-letter tests pass unchanged.
- [x] A handler that succeeds after its job was dead-lettered still removes the job (`complete` unchanged).
- [x] `make check` passes.

## Resolution

`Worker::fail`'s `UPDATE` now also requires `dead_at IS NULL` and reads `rows_affected()`: with no row changed, it leaves the dead letter (and its `last_error`) as it is and warns "job failed after it was dead-lettered; left dead"; otherwise it logs the retry or dead-lettering after the update. `complete` is unchanged. Tests in `crates/common/db/tests/delivery/effects.rs`: `a_delivery_that_fails_after_its_subscription_was_retired_stays_dead` and `a_delivery_that_succeeds_after_its_subscription_was_retired_is_removed`. ADR 0013 and 0014 updated.
