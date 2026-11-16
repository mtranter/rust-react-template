---
id: 0023
name: Retiring a subscription takes its locks in the order registering one does, so the two can't deadlock
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: PR 3 review (Copilot, on migration 0008: concurrent retirement and `earliest` registration can deadlock)
---

# Retiring a subscription takes its locks in the order registering one does, so the two can't deadlock

## Description

`register_subscriptions` locks `messaging.subscription` in `SHARE ROW EXCLUSIVE` mode, then, when it registers a new `earliest` subscription, takes the relay's advisory lock to record the backfill. `messaging.retire` (migration 0008) took the relay's lock first and then updated `messaging.subscription`, which needs a lock the registration's conflicts with. Two `migrate` runs overlapping, one retiring and one registering, could each hold the lock the other waited for. Postgres aborts one after `deadlock_timeout`; both are transactional, so a rerun succeeds, but a `migrate` failing on a deadlock in a rollout is avoidable.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/subscriptions.rs` `register_subscriptions`: `LOCK TABLE messaging.subscription IN SHARE ROW EXCLUSIVE MODE`, then `backfill` takes `RELAY_LOCK`.
- `crates/common/db/migrations/0008_retire_function.sql`: `pg_advisory_xact_lock` first, then `UPDATE messaging.subscription`.

## Proposed approach

A new migration replaces the function (0008 is applied and immutable) with the same body preceded by `LOCK TABLE messaging.subscription IN SHARE ROW EXCLUSIVE MODE`. The relay only reads the table, which that mode allows, so it is unaffected; the reasoning in ticket 0015 holds unchanged, since the relay's lock is still held from before the first `UPDATE` until commit.

## Acceptance criteria

- [x] A test: while a registration holds the subscription table's lock, a retirement waits for it without holding the relay's lock.
- [x] ADR 0014 states the lock order; `make check` passes.

## Resolution

`0009_retire_locks_subscriptions_first.sql` replaces `messaging.retire`. Test: `retiring_waits_for_a_registration_without_holding_the_relays_lock` in `crates/common/db/tests/delivery/deploys.rs` (fails against 0008: the relay's lock is held while the retirement waits). ADR 0014's retirement paragraph gives the order.
