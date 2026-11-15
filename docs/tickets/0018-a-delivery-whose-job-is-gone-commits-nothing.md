---
id: 0018
name: A delivery whose job another worker already completed commits nothing
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: evaluation of the pub-sub mechanism 2026-11-15 (found by reading; not reported by Copilot)
---

# A delivery whose job another worker already completed commits nothing

## Description

`Worker::complete` runs the handler and deletes the job in one transaction, so "a handler that only writes to the database takes effect exactly once". That holds only while the handler finishes within its lease (5 minutes). If it runs longer, the job becomes due again and a second worker claims and runs it while the first is still going (`LEASE`'s doc: "may run twice at once"). Both transactions then commit the handler's writes, because `complete` never checks that its `DELETE` removed a row:

1. Worker A's handler is slow. Its lease runs out.
2. Worker B claims the same job, runs the handler, deletes the job, commits.
3. Worker A's handler finishes. Its `DELETE FROM messaging.job WHERE id = $1` affects 0 rows. It commits anyway, applying the handler's writes a second time.

Projections survive this (their version guard makes the second apply a no-op), but a reaction that writes to the database (archiving notes, inserting a row) is applied twice. The check costs one comparison and makes database effects exactly-once even past the lease.

## Evidence (as of 2026-11-15)

`crates/common/db/src/worker.rs` `complete`:

```rust
sqlx::query!("DELETE FROM messaging.job WHERE id = $1", job.id)
    .execute(conn(&mut tx))
    .await
    .map_err(AppError::internal)?;
tx.commit().await
```

The result of `execute` is discarded.

## Proposed approach

In `complete`, read `rows_affected()` of the delete. If it is 0, the job was completed by another run: do not commit. Drop the transaction (dropping an uncommitted `Tx` rolls it back; `TxBackend` has only `commit`), log at warn with the queue ("delivery already completed by another worker: its lease ran out, so this run's writes are rolled back"), and return `Ok(())`.

Return `Ok`, not an error: an error would send `run_next` into `fail`, which would log a misleading "job failed; will retry" for a job that no longer exists.

Why the delete is a sound test:
- If the other run has deleted the row but not yet committed, this delete waits on its row lock. When the other commits, this one finds no row (0 affected) and rolls back. If the other rolls back instead (its handler failed), this one deletes the row and commits: still exactly one application.
- The delete must stay unconditional on `dead_at`: a job dead-lettered while its slow handler was still running (ticket 0016) is rightly removed when that handler succeeds.

Update the doc comments: `complete` ("takes effect exactly once" now holds past the lease too) and `LEASE` (a handler that outruns it may still *run* twice at once, but only one run's database writes commit; effects outside the database, such as mail, can still happen twice, which is why subscribers stay idempotent).

## Acceptance criteria

- [x] A test in `crates/common/db/tests/delivery/`: a handler that writes a row through `app_db::conn(tx)` and, while running, has its job row deleted from another connection (standing in for a second worker that completed it) leaves no row behind: its write is rolled back, `run_next` returns `Ok(true)`, and no job remains or is marked failed. A test-local `Handle` implementation holding a `PgPool` can do the delete inside `handle`; `with_copy_table` in `projection.rs` shows how tests create a table to write to.
- [x] A normal delivery still commits its handler's writes and removes its job; all existing delivery tests pass unchanged.
- [x] Doc comments on `complete` and `LEASE` match.
- [x] `make check` passes.

## Resolution

Done as proposed. `Worker::complete` reads `rows_affected()` of its `DELETE`; when it is 0 it logs at warn with the queue, drops the `Tx` uncommitted (rolling back the handler's writes) and returns `Ok(())`, so `run_next` reports the delivery as run and never calls `fail`. The delete stays unconditional on `dead_at`; the SQL is unchanged, so `.sqlx` is too. Doc comments on `complete` and `LEASE` updated, and ADR 0013's lease sentence gained one line citing this ticket.

Tested in the new `crates/common/db/tests/delivery/effects.rs` (`a_delivery_another_worker_completed_meanwhile_commits_none_of_its_writes`), whose handler writes a row and deletes its job from another connection. The normal path (a delivery commits its writes and removes its job) is already covered by the existing projection and order tests, which pass unchanged, so no new test was added for it: one could not have gone red.
