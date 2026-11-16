---
id: 0019
name: A backfill with nothing to copy is complete when `migrate` registers it, so adding an event type doesn't make a served projection unavailable
status: done
priority: P2
size: S
blocked_by: [0013]
blocks: []
source: evaluation of the pub-sub mechanism 2026-11-15 (found by reading; not reported by Copilot)
---

# A backfill with nothing to copy is complete when `migrate` registers it, so adding an event type doesn't make a served projection unavailable

## Description

A whole-topic `earliest` subscription has one row in `messaging.subscription`, and so one in `messaging.backfill`, per event type of its topic. When a topic gains an event type, the next `migrate` inserts a subscription row for the new type under every existing projection queue, with a new backfill row whose `completed_at` is null. `caught_up` requires every backfill row of a queue to be complete, so the projection stops counting as caught up until a worker next polls and completes that (necessarily empty) backfill.

Server processes already running don't notice, because `Generations` has latched. But the same deploy starts new server processes, and each of those answers `unavailable` (503) for the read model until a worker has polled. A projection that has been serving for a year goes briefly dark on a deploy that only added an event type. If no worker of any build is running at that moment, it stays dark until one is.

The backfill of a brand-new event type has nothing to copy: no event of that type can exist before the build that publishes it. `migrate` can see that and record the backfill as complete at once.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/subscriptions.rs` `register_subscriptions`: a newly inserted `(queue, event_type)` row that starts from `earliest` gets a backfill row; `backfill` inserts it with `completed_at` null in every case.
- `crates/common/db/src/worker.rs` `copy_chunk`: only a worker sets `completed_at`, on finding an empty chunk.
- `crates/common/db/src/projection.rs` `caught_up`: `b.completed_at IS NULL` on any row of the queue means not caught up.

## Proposed approach

In `subscriptions.rs` `backfill`, set `completed_at` in the `INSERT INTO messaging.backfill` when the backfill has nothing to copy: no outbox event of that row's `topic` and `event_type` with `relay_batch <= through_batch`. That is the same predicate `copy_chunk` uses for its chunk, evaluated from `after_seq = 0`, so the two agree on what "nothing to copy" means. It is evaluated under the relay's lock, in the statement that reads `through_batch`, so no relay can dispatch such an event in between. Use the `clock.now()` already passed to that statement for the timestamp, and log `backfill complete` for those rows as the worker does.

`outbox_backfill_idx (topic, event_type, seq)` serves the lookup.

This is only correct together with ticket 0013. After 0013, readiness also waits for events at or below `through_seq` that are undispatched or in unhandled jobs, so a backfill completed at registration still isn't "caught up" while events published before registration are waiting to be relayed. Without 0013, completing at registration would make the premature-readiness bug worse. Hence `blocked_by: [0013]`.

### Behaviour change to carry through

A projection registered against a topic whose outbox is empty (a fresh deployment, or most tests) is now caught up as soon as `migrate` finishes, with no worker run. That is correct by the definition of caught up (it holds everything the topic held at registration: nothing), but it changes things that assumed a 503 first:

- `nothing_is_served_until_a_generation_has_caught_up` in `crates/common/db/tests/delivery/projection.rs` registers against an empty outbox and expects `Unavailable`. Give it history (publish and relay an event before registering), so it still shows the 503-then-served sequence.
- Check the other tests in that file and in `earliest.rs` that count `backfill()` calls or assert on `completed_at`.
- Check `note-search`'s tests and driver, the composed-app test and the journeys for an expectation that search is unavailable before the worker first runs, and the PR/ADR 0036 sentence that a fresh deployment answers 503 until its worker has run once. Seeds are published after `migrate`, so they are live events: the copy is "caught up" and empty until the worker delivers them, like any other write to an eventually consistent read model.
- The frontend's "Search is still catching up" state stays: it still applies while a real backfill runs.

## Acceptance criteria

- [x] A test: a caught-up projection whose queue then gains a subscription to a further event type of its topic (register handlers for one event type from `earliest`, catch up, then register the same queue for a second type) is still caught up immediately after that registration, with no worker run in between.
- [x] A test: a new `earliest` subscription to a topic with no relayed events of its type is caught up straight after registration, and a later `Worker::backfill` copies nothing for it.
- [x] A test: a new `earliest` subscription to a topic that has relayed events is *not* complete at registration and is copied by the worker as today.
- [x] With events published but not yet relayed at registration (and none relayed), the backfill is complete at registration but `caught_up` is false until they are relayed and handled (ticket 0013's rule).
- [x] Tests and docs that assumed a 503 before the first worker run on an empty topic are updated; ADR 0035 says `migrate` completes an empty backfill itself.
- [x] `.sqlx` regenerated; `make check` passes.

## Resolution

`subscriptions.rs` `backfill` now writes `completed_at` (the registering clock's `now`) in the same `INSERT`, under the relay's lock, for a row with no outbox event of its topic and type at `relay_batch <= through_batch`, the predicate `copy_chunk` copies by, and logs `backfill complete` for each such row. `through_batch` and `through_seq` moved into a CTE so the predicate can use them.

Tests: `a_caught_up_projection_stays_caught_up_when_its_queue_subscribes_to_another_event_type` and `a_projection_registered_before_any_event_is_relayed_waits_only_for_those_events` (`projection.rs`), `a_backfill_with_no_relayed_events_of_its_type_is_caught_up_once_registered` (`earliest.rs`). A backfill with relayed events staying incomplete is already covered by `a_projection_has_caught_up_once_its_backfill_is_copied_and_handled` and `registering_copies_nothing_and_a_worker_copies_the_backfill`, so no new test was added for it (15 existing tests fail if every backfill completes at registration). `nothing_is_served_until_a_generation_has_caught_up` now registers against a relayed event. The note-search and widgets drivers, the composed-app test and the journeys needed no change: both drivers relay the seeds before registering, so their copies still have a real backfill, and no journey expects a 503. ADR 0035 says `migrate` completes an empty backfill; ADR 0036's fresh-deployment consequence and `crates/AGENTS.md` are updated.
