# The worker copies an `earliest` backfill in chunks

Amends [ADR 0033](0033-the-outbox-is-a-compacted-log.md)'s "A subscription declares where it starts"; the rest of it stands.

ADR 0033 had `migrate` backfill a new `earliest` subscription in the transaction that registers it: it inserted one job per relayed outbox event of the subscription's topic and types in one statement, while holding the relay's advisory lock. At template scale that takes milliseconds. A module added years later to keep a copy of a topic with millions of keys would have its deploy wait on one huge insert, and no event of any topic would be relayed until it finished. Keeping a copy of another module's state (a search projection, a cache) is what `earliest` exists for, so the backfill has to scale with the topic.

**`migrate` only records the backfill.** Still under the relay's lock, it writes a `messaging.backfill` row per new `earliest` subscription. The row records the last relay batch committed so far (`through_batch`) and the outbox's highest `seq` (`through_seq`). The backfill's events are its topic's and types' events relayed in that batch or an earlier one; every later event goes to the new queue through the relay, as before. `through_seq` bounds what the copy must catch up with: every event its topic held at registration, relayed or still waiting for the relay, has a `seq` at or below it (ticket 0013). The lock is held for one small insert.

**Relay batches are numbered in commit order.** The relay takes a number from `messaging.relay_batch` under its lock and stamps it on every event it dispatches (`outbox.relay_batch`). The outbox's `seq` can't mark the boundary: it is assigned at insert, not at commit, so a lower `seq` can commit after a higher one (ADR 0013). Only one relay runs at a time, and it holds the lock until it commits, so batch numbers follow commit order. Events relayed before batches were numbered belong to batch 0.

**Any worker copies any backfill, a chunk at a time.** `Worker::backfill` locks one unfinished backfill with `FOR UPDATE SKIP LOCKED` and copies its next 1000 events, in `seq` order, into `messaging.job`. In the same transaction it moves the backfill's cursor (`after_seq`) past them. A worker that dies loses at most its uncommitted chunk. Two workers never copy the same backfill at once, and neither waits for the other. A backfill with nothing left to copy is marked `completed_at`. A worker copies between relaying and running jobs, so a backfill never stops live delivery.

**A key's live events still wait for its backfilled ones.** A claim already waits for a job of an earlier version of its key in its queue. It now also waits for an earlier version of its key that its queue's unfinished backfill has yet to copy. The cursor moves in the transaction that inserts the copied jobs, so a claim sees either an uncopied event or its job, never neither. Each key is still delivered in version order with backfilled events first, and other keys' live events don't wait. ADR 0033's guarantees hold unchanged.

**Compaction can't open a gap.** The backfill's events are fixed when it is registered, so nothing is added to them later. Compaction deletes an event only when a later version of its key exists. That later version is still to be copied, or has been relayed to the queue, so each key's latest state still arrives.

## Considered options

- **A `seq` high-water mark.** Simpler, but `seq` isn't commit order: an event inserted before the mark but committed after it would be missed by both the backfill and the relay.
- **Copying from the providers' Published Views instead of the outbox** (a version-guarded snapshot plus a `latest` subscription). It works without event history, but it is per-module SQL, depends on views a lifted-out module no longer has, and needs `version` in every view. The compacted outbox already holds every key's latest state, in one shape for every topic.
- **Running the backfill's events through the handler directly**, without jobs. It would skip `messaging.job`'s growth, but also its retries, dead letters and per-key ordering, which every handler relies on.
- **Holding every live job of a queue until its backfill completes.** Simpler to claim, but one slow backfill would freeze every key's live updates rather than only the keys it hasn't reached.
- **A separate backfill process, or `migrate` as an init container.** A long backfill in either would block a rollout or need its own scheduling. The worker already polls, and the database lock makes concurrent copying safe, as it does compaction.

## Consequences

- Registering an `earliest` subscription is fast however large its topic. The copy runs in the background, and `messaging.backfill` shows its progress (`after_seq` against the topic's `seq`) and when it completed.
- A copy built from `earliest` is incomplete until it has caught up: every event its topic held at registration, relayed then or not, is handled. [ADR 0036](0036-modules-keep-event-fed-projections.md) serves one only once it has caught up (`app_db::caught_up`).
- A claim joins the outbox for any queue with an unfinished backfill: a cost only while a backfill runs, and indexed by `outbox_event_idx`.
- The backfill is copied 1000 events per transaction unless `Worker::backfilling_in_chunks_of` says otherwise; there is no throttle.
- Rebuilding a copy (a new index shape, a new column) is a new queue from `earliest` beside the old one, switched over once it completes: `app_db::Generations` (ADR 0036).
