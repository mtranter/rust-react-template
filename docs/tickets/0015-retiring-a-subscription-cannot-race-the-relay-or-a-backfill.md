---
id: 0015
name: Retiring a subscription is one function call that cannot race the relay or a backfill
status: done
priority: P2
size: M
blocked_by: []
blocks: []
source: PR 3 review (Copilot, "Lock subscription rows during backfill to serialize with retirement"); the relay half found in the follow-up evaluation 2026-11-15
---

# Retiring a subscription is one function call that cannot race the relay or a backfill

## Description

Retiring a subscription is a hand-written migration of two statements (documented in `0001_messaging.sql`, `crates/AGENTS.md` and ADR 0014): mark it retired, then dead-letter its unrun jobs. Run while workers are up, it can leave **live jobs in a retired queue**, in two ways. A worker of an older build that still has the handler will then run them after retirement (one more email sent, or writes to a projection's tables that are about to be dropped); on newer builds they sit in `messaging.job` forever.

**Race with a backfill chunk** (the one Copilot reported). `Worker::copy_chunk` reads `retired` in its first statement, then inserts a chunk of jobs. If the retirement commits between the two, its `UPDATE messaging.job` ran before the chunk's jobs existed, so they stay live.

**Race with the relay** (same shape, not reported). `Worker::relay` inserts jobs from the `messaging.delivery` view, which filters `retired_at IS NULL` in that statement's snapshot. If the retirement is in progress but uncommitted, the relay still fans out to the queue; the retirement's `UPDATE messaging.job` cannot see the relay's uncommitted jobs, so they stay live.

Both are narrow windows in a rare manual operation, hence P2. But the recipe is copied by hand into migrations, so it should be correct by construction.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/worker.rs` `copy_chunk`: `FOR UPDATE OF b SKIP LOCKED` locks only the backfill row; `s.retired_at` is read unlocked from the same statement's snapshot.
- `crates/common/db/src/worker.rs` `relay`: holds `pg_try_advisory_xact_lock(RELAY_LOCK)` until commit; nothing in the retire recipe takes that lock.
- `crates/common/db/tests/delivery/support.rs` `Messaging::retire`: the recipe as two autocommit statements.

## Proposed approach

Ship the recipe as a SQL function, `messaging.retire(queue text)`, that serialises itself against both, and make a retire migration one line: `SELECT messaging.retire('<queue>');`.

### The function (new migration, next free number in `crates/common/db/migrations/`)

`LANGUAGE plpgsql`, `VOLATILE` (the default), statements in this order:

1. `PERFORM pg_advisory_xact_lock(491327873401);` This is `RELAY_LOCK` (`0x72_656c_6179`, "relay" in ASCII) from `worker.rs` as a decimal. It waits for a relay in flight to commit, and holds off the next until the migration's transaction commits (a relay that can't take the lock returns 0 and tries at its next poll).
2. `UPDATE messaging.subscription SET retired_at = now() WHERE queue = $1 AND retired_at IS NULL;`
3. `UPDATE messaging.backfill SET completed_at = now() WHERE queue = $1 AND completed_at IS NULL;`
4. `UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired' WHERE queue = $1 AND dead_at IS NULL;`

It must be plpgsql with separate statements, not one SQL statement with CTEs: under READ COMMITTED each statement of a volatile plpgsql function takes a fresh snapshot, which the reasoning below depends on.

Add a comment in the migration saying it supersedes the two-statement recipe in `0001_messaging.sql` (that file is applied and immutable, so its comment can't be corrected).

### Why it closes both races

**Relay.** The relay holds the advisory lock from before it reads its batch until it commits. Step 1 waits for that, so step 4's snapshot sees the relay's jobs. A relay starting while the function's transaction is open fails its try-lock; one starting after it commits sees `retired_at` set.

**Backfill.** Step 3 is what matters, case by case:
- *A chunk is being copied* (the worker holds the row lock on the backfill row). Step 3 waits for that lock, so it runs after the chunk commits, and step 4 sees the chunk's jobs.
- *The function runs first and is uncommitted.* The worker's `SKIP LOCKED` skips the locked backfill row.
- *The function commits between the worker's snapshot and its attempt to lock the row.* The row was updated, so Postgres re-evaluates the worker's `WHERE b.completed_at IS NULL` against the new row version, which now fails, and the row is skipped. This is the case a plain lock on the backfill row (without updating it) would not close, because the worker would still read `retired = false` from its older snapshot.

A backfill of a retired subscription completing "with nothing copied" is already the worker's behaviour; step 3 only does it sooner. Leave the `retired` branch in `copy_chunk` as it is: it still covers a subscription retired by the old recipe.

### Call sites and docs

- `Messaging::retire` in `crates/common/db/tests/delivery/support.rs` calls the function.
- `crates/AGENTS.md`, the "Retire one with a migration" block: replace the two `UPDATE`s with the function call; keep the separate `DELETE FROM messaging.projection_version` line for a projection's queue, which is still run later, once nothing serves its tables.
- ADR 0014 (and ADR 0036 if it repeats the recipe): describe retirement as the function; note why it takes the relay lock.
- `GLOSSARY.md` if it describes retiring.

## Acceptance criteria

- [x] `SELECT messaging.retire('<queue>')` retires the subscription, completes its unfinished backfills and dead-letters its live jobs; the existing test `retiring_a_subscription_dead_letters_what_it_hadnt_run_and_relays_it_nothing_more` passes through it.
- [x] A test proves it shares the relay's lock: with events waiting and `messaging.retire` called inside an open, uncommitted transaction, `Worker::relay` returns 0; after that transaction commits, a relay dispatches the events and creates no job for the retired queue.
- [x] A test proves it waits for a chunk in flight: a transaction locks the queue's `messaging.backfill` row `FOR UPDATE` and inserts a job for the queue (as `copy_chunk` does); `messaging.retire` called from another connection does not return until that transaction commits, and afterwards that job is dead-lettered. (Assert the wait with a short `tokio::time::timeout` on the retire future before committing.)
- [x] After retirement, `Worker::backfill` copies nothing for the queue and `messaging.job` holds no live job for it.
- [x] The docs above describe the function, and no doc that can still be edited shows the two-statement recipe.
- [x] `make check` passes.

## Notes

Copilot's suggested fix (lock the subscription row in `copy_chunk`) closes the backfill race but not the relay one, and makes the worker's query carry the burden. Putting the serialisation in the function leaves both worker queries unchanged.

## Resolution

Migration `0008_retire_function.sql` adds `messaging.retire(queue)` as proposed: plpgsql, taking the relay's advisory lock, then retiring the subscription, completing its unfinished backfills and dead-lettering its live jobs, each a statement of its own. `Messaging::retire` in the delivery tests calls it; `crates/AGENTS.md`, ADR 0014 and ADR 0036 describe it (ADR 0014 says why it takes the relay lock and updates the backfill); `GLOSSARY.md` doesn't describe retiring, so it is unchanged. `RELAY_LOCK` in `worker.rs` notes that the function takes it as a decimal. Both worker queries are unchanged.

Tests: `no_relay_runs_while_a_retirement_is_uncommitted_and_the_next_relays_nothing_to_the_retired_queue` (deploys), `retiring_waits_for_a_chunk_being_copied_and_dead_letters_it` and `retiring_completes_the_backfill_and_copies_nothing_more` (earliest). The last also asserts the backfill is complete straight after `retire`, before any worker runs, since the worker's `retired` branch alone would copy nothing too.

The docs claim only the serialisation: a job a worker had already claimed is dead-lettered, but `Worker::fail` makes it live again if its handler then fails. That is ticket 0021, found in review.
