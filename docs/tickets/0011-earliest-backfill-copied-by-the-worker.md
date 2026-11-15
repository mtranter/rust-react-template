---
id: 0011
name: An `earliest` subscription's backfill is copied by the worker in chunks, not by `migrate` in one transaction
status: in-progress
priority: P2
size: M
blocked_by: []
blocks: []
source: conversation 2026-11-15 (adding a search projection to a module that has been in production for a year)
---

# An `earliest` subscription's backfill is copied by the worker in chunks, not by `migrate` in one transaction

## Description

A module added long after another has been in production, such as a search read module keeping its own indexed copy of notes, needs every existing root and not only the events from now on. ADR 0033 already provides this: the outbox is a compacted log holding every key's latest event, and an `earliest` subscription is backfilled from it. No CDC snapshotting (DBLog and similar) is needed. But `migrate` does the backfill in one statement inside its registration transaction, while holding the relay's lock. With millions of keys the deploy waits on it, and nothing is relayed until it finishes.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/subscriptions.rs` `backfill`: `INSERT INTO messaging.job SELECT … FROM messaging.delivery WHERE dispatched_at IS NOT NULL` for the whole topic, under `pg_advisory_xact_lock(RELAY_LOCK)`, in `migrate`'s transaction.

## Proposed approach

ADR 0035: `migrate` records a `messaging.backfill` row bounded by the last committed relay batch, and workers copy it into the queue 1000 events per transaction. A claim holds a key's live event back until the key's earlier events are copied.

## Acceptance criteria

- [x] Registering an `earliest` subscription inserts no jobs; a worker's `backfill` copies them.
- [x] Every existing `earliest` delivery test passes unchanged: version order per key, backfilled before live, other keys not held up, exactly once while registering concurrently with the relay.
- [x] ADR 0035 written; ADR 0033, `crates/AGENTS.md` and the glossary point at it.
- [ ] Tests for a backfill spanning several chunks, a worker dying mid-backfill, and two workers backfilling at once (needs a configurable chunk size).
- [x] `make check` passes.

## Notes

Follow-ups, not in this ticket: a `Projection` helper and a `make new-projection` scaffold for read modules; a readiness check on `completed_at`; rebuilding a copy through a `_v2` queue; moving `note-search` onto a projection as the reference.
