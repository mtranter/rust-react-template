---
id: 0022
name: "Compaction leaves an event an unfinished backfill has yet to copy, so a projection is never served without a key its topic held at registration"
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: "PR 3 review (Copilot, on `caught_up`: compaction removing an uncopied registration-time event)"
---

# Compaction leaves an event an unfinished backfill has yet to copy, so a projection is never served without a key its topic held at registration

## Description

`caught_up` promises that a served copy holds every key its topics held when `migrate` registered it. Compaction could break that for one key at a time.

Take an event E of key K, relayed before registration and older than its topic's retention, that the backfill has not copied yet. A newer version of K is published after registration and relayed. Compaction now sees a later version of K and deletes E. The backfill reaches its end without K and completes. The newer version's job has an `outbox_seq` above the backfill's `through_seq`, so `caught_up` doesn't wait for it, and `Generations` can serve the copy while K is missing from it. A generation that replaced an older one would show a key the older one had as absent.

The copy heals as soon as the newer version's job is handled, so the window is short. ADR 0035's "compaction can't open a gap" held for eventual delivery but not for readiness.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/worker.rs` `compact`: deletes a dispatched event past retention whenever a later version of its key exists, with no regard to backfills.
- `crates/common/db/src/projection.rs` `caught_up`: waits only for undispatched events and live jobs at or below `through_seq`.
- `crates/common/db/src/worker.rs` `run_next`: holds a key's live job back only while an uncopied earlier event of the key exists in the outbox, so once E is deleted nothing ties the newer job to the backfill.

## Proposed approach

In `compact`, skip an event that an unfinished backfill of its topic and event type has yet to copy: `seq > after_seq AND relay_batch <= through_batch`, the predicate `copy_chunk` and `run_next` already use. The event is compacted at the next pass after it is copied, since jobs are copies. A retired subscription's backfill is complete, so it protects nothing.

Rejected: making `caught_up` wait for the replacement. It would have to follow keys across the boundary, where the rule today is one comparison with `through_seq`.

## Acceptance criteria

- [x] A test: an event past its retention, replaced after an `earliest` subscription was registered and before its backfill copied it, survives compaction and is delivered to the copy before its replacement.
- [x] A test: the same event is compacted once the backfill has copied it, while the backfill is still unfinished.
- [x] ADR 0035 and `crates/AGENTS.md` state the rule; `.sqlx` regenerated; `make check` passes.

## Notes

One interleaving is left open, accepted as too narrow to justify a lock in compaction. A compaction batch whose snapshot falls between a registration's `INSERT INTO messaging.backfill` and its commit doesn't see the backfill row. If a newer version of a key also commits inside that same gap (after the registration read `through_seq`, before the compaction's snapshot), the older event can still be deleted. The gap is the last statement of `register_subscriptions` to its commit, and the effect is the same short-lived missing key.

## Resolution

`Worker::compact` gained the `NOT EXISTS` over `messaging.backfill`. Tests in `crates/common/db/tests/delivery/earliest.rs`: `compaction_keeps_an_event_an_unfinished_backfill_has_yet_to_copy` (fails without the change: compaction deletes 1) and `compaction_deletes_an_event_once_an_unfinished_backfill_has_copied_it`. ADR 0035's "Compaction can't open a gap" and the compaction rule in `crates/AGENTS.md` say so.
