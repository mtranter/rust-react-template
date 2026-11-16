---
id: 0009
name: Deliver events by topic, in order per key, and safely across rolling deploys
status: done
priority: P1
size: L
blocked_by: []
blocks: []
source: discussion 2026-11-01 (event delivery ordering, topics and multi-instance safety)
---

# Deliver events by topic, in order per key, and safely across rolling deploys

## Description
The outbox and worker (0007) deliver every event at least once, in no order, and
only while every running process has the same build. This ticket changes two
things:

- **Delivery becomes ordered per entity.** A subscriber receives the events of
  one entity in the order they happened. This is the guarantee the app needs.
  Global order is not needed.
- **Delivery stays correct while old and new builds run side by side** during a
  rolling deploy.

The model is Kafka's:

| Kafka | Here |
|---|---|
| Topic | One per aggregate root, declared in its module's `-api` (`users.user.v1`) |
| Message key | The root's id |
| Order within a partition | Order per key, by the root's `Version` |
| Consumer group | A subscription, i.e. a queue (`notes.archive_deleted_authors_notes`) |
| Offset | A `messaging.job` row per message still to deliver, deleted once handled |

There are two deliberate differences:
- **No partitions.** Order is enforced per key, which is finer than Kafka's per
  partition. A failing message holds up only its own key.
- **No offsets.** The relay creates jobs instead of each subscriber tracking a
  position in the outbox. Offsets need the outbox in commit order, and `seq` is
  not commit order. Order per key doesn't need commit order across keys (see
  "Why `Version` can order a key").

### What goes wrong today
1. **An old worker dead-letters a new subscription's jobs.** `run_next` claims
   any due job, whatever its queue. An old worker that claims a job for a queue
   added in the new build fails it with `NoHandler`, and the claim has already
   used up one of the job's `MAX_ATTEMPTS` (5). With backoff of 10s to 160s, a
   rollout lasting a few minutes can dead-letter real work.
2. **An old relay drops a new subscription's deliveries for good.** `relay`
   fans out using its own process's `Handlers::subscribers`, then marks the event
   dispatched. If an old worker relays an event, no job is ever created for a
   subscription it doesn't know about. The reverse happens when a subscription is
   removed: a new relay stops creating its jobs while old workers would still run
   them.
3. **Nothing is ordered, and consumers can't tell a newer state from an older
   one.** Three things reorder events:
   - the outbox `seq` comes from a sequence at INSERT time, not commit time;
   - several workers claim jobs at once;
   - a failed job backs off while later jobs run.

   Events carry the root's whole state but not its version. So a subscriber that
   keeps a copy of that state can overwrite version 4 with a late version 3.
4. **A subscriber can't see one entity's events in order, even in principle.**
   Each queue handles exactly one event type. A consumer of `UserRegistered`,
   `UserDisplayNameChanged` and `UserDeleted` needs three queues, each delivered
   independently.
5. **`EventEnvelope::time` is misdocumented.** Its doc says it is when the unit
   of work committed. It is actually `clock.now()` at publish, before the commit,
   on whichever instance ran the transaction. It must not be used to order
   events.

### Why `Version` can order a key
Every write to an aggregate root is a version-guarded UPDATE or DELETE on the
root's row (§4). So v4's transaction waits on the row lock until v3's commits.
The outbox row is written in the same transaction. Two things follow for any
one key:
- **Commit order is version order.**
- **A relay statement that can see v4's outbox row can also see v3's.**

That holds only if the relay runs one statement at a time and the claim waits
for lower versions. Commits 4 and 5 make both true.

## Evidence (as of 2026-11-01, `main` at `38560c2`)
- `crates/common/db/src/worker.rs`:
  - `run_next`: the claim's `WHERE dead_at IS NULL AND run_at <= $1` has no queue
    filter. An unknown queue fails in `Handlers::handle`
    (`crates/common/persistence/src/messaging.rs`, `NoHandler`) after the claim
    has already counted as an attempt.
  - `relay`: it loops over `self.handlers.subscribers(&event.event_type)`, then
    sets `dispatched_at` for the whole batch. It locks the batch with
    `FOR UPDATE SKIP LOCKED`, so two relays can each take part of one key's
    events and commit them out of order.
- `crates/common/db/migrations/0001_messaging.sql`: `messaging.outbox.seq` is
  `bigserial`. Neither the outbox nor the job table records a key or a version,
  and there is no subscription table.
- `crates/common/events/src/lib.rs`: `Event` has a `TYPE` (`<module>.<event>.v<n>`),
  `SOURCE`, `Entity`, `Context` and `subject`. Nothing groups a root's events.
- `crates/common/persistence/src/messaging.rs`: `Handlers` maps one queue to one
  event type, and registering a second handler for a queue panics.
- `crates/common/events/src/envelope.rs:21`: the `time` doc comment.
  `crates/common/db/src/publishing.rs:25`: `let now = self.clock.now();`.
- `crates/modules/users/core/src/application/service.rs`, `delete`: the event
  announces the version deleted. That is the same version as the last
  `UserDisplayNameChanged`, so the two would tie on any version-based order.
- The event state types (`users_api::events::v1::User`,
  `notes_api::events::v1::Note`, the template's `v1::Widget`) have no version.

## Proposed approach
Five commits, in this order, each leaving `make check` green. Commits 1–2 can
ship on their own if this has to be split.

### 1. Workers claim only what they can decode
- `Handlers` lists the `(queue, event_type)` pairs it serves.
- Add `event_type` to `messaging.job`, filled in by the relay.
- The claim takes only jobs whose `(queue, event_type)` is one of the worker's
  own: `AND (queue, event_type) IN (SELECT * FROM unnest($n::text[], $m::text[]))`.
- A job that no running worker can decode waits instead of failing.
  - Say so in the `NoHandler` doc comment and in spec §4.
  - `NoHandler` stays as a guard.
- Run `EXPLAIN` on the claim, and adjust `job_due_idx` if the plan needs it. Jobs
  that no live worker serves sit at the front of `run_at` order, so test the plan
  with some of them present.
- **This doesn't protect the deploy that ships it,** because the build being
  replaced still claims any queue. Don't add a subscription in the same release.

### 2. Subscriptions live in the database, registered at migrate time
- Add `messaging.subscription`:
  `(queue text, event_type text, topic text NOT NULL, registered_at timestamptz NOT NULL, retired_at timestamptz, PRIMARY KEY (queue, event_type))`.
  - Commit 3 gives `topic` its meaning. Until then, store the event type's
    module (`SOURCE`).
- **Register when migrating, not when a worker starts.**
  - The `migrate` bin calls `app_db::register_subscriptions(&pool, &handlers)`
    with every module's handlers, after the migrations.
  - Migrations run before a rollout (§6), so every subscription in the new build
    exists before any new process publishes or relays. No window exists in which
    events go out without their new subscribers.
  - Registration needs only names and types, so collect it without building any
    services (for example a `Handlers` listing, or a `Module` method).
  - Test drivers call the same function when they build their worker in
    `seeded`.
- **The worker checks the table at startup, and never writes to it.**
  - It refuses to start if one of its own pairs is missing, or is registered
    under a different topic.
  - It logs a warning for each live subscription that it doesn't serve.
  - It still runs pending jobs of a retired subscription it serves.
- **The relay fans out in SQL:** it joins the batch to the subscriptions with
  `retired_at IS NULL`.
- **Semantics** (write them in spec §4):
  - A subscription receives every event relayed after its registration commits.
    It receives no history (see Out of scope).
  - A queue is bound to one topic forever. Registering an existing queue under
    another topic fails the migrate step loudly. Moving to a new topic version
    means a new queue name.
  - A retired queue name is never reused. Registering one fails the migrate step.
    This stops a rollback, or an old migrate, from bringing a subscription back.
- **Retiring a subscription** is a migration that does two things in one
  statement each:
  - sets `retired_at`;
  - dead-letters the subscription's pending jobs, with `last_error = 'subscription retired'`.

  The jobs are dead-lettered, not deleted, so they stay visible. Retiring never
  happens as a side effect of deploying code without the handler. Document the
  SQL in spec §4.

### 3. Topics, with an exhaustive event enum per topic
- Add `app_events::Topic`:
  - `NAME` (`users.user.v1`), `SOURCE`, `Entity`, `Event` (the enum below) and
    `key(&Entity) -> String`;
  - a constant name for the entity's type, if `NAME` alone doesn't serve for
    the outbox row.
- **Each topic's `-api` declares a plain enum of its events.**
  - One variant per event type. A variant holds that event's context: none, or
    e.g. `ItemRemoved { item_id }`.
  - The enum is deliberately **not** `#[non_exhaustive]`. A new variant must
    break every whole-topic consumer's build, in the same change.
- **Versioning moves to the topic.**
  - Event types become `users.user_deleted`.
  - Removing a variant, or changing the entity incompatibly, means a new topic
    (`users.user.v2`), published alongside v1 until its consumers have moved.
  - Adding a variant is additive on the wire.
  - Contexts and entity state only gain fields, each `#[serde(default)]`.
- **The wire format stays CloudEvents.**
  - `type` is the event type, and `subject` is the key.
  - New extension attributes: `topic`, `partitionkey` (from CloudEvents'
    partitioning extension) and `entityversion` (commit 4).
  - `data` stays `{ entity, context? }`.
  - Decoding has two steps:
    1. read the envelope with `type` as a string and `context` as raw JSON;
    2. `UserEvent::decode(type, context)` picks the variant by type string and
       parses its context.

    A serde-tagged enum can't express this, because `type` and the context sit
    at different levels of the JSON.
- **One declarative macro per topic** (like `uuid_newtype!` and
  `json_newtype!`) generates:
  - the `Topic` impl and the enum;
  - `decode` and `encode`;
  - a fieldless `UserEventKind` holding each variant's type string;
  - one marker type per event (`UserDeleted`) implementing `Event` with
    `type Topic` and its `Context`;
  - a unit test that the type strings are unique within the topic.

  The macro's syntax is the implementer's call. Keep it as small as
  `uuid_newtype!`.
- `Event` shrinks to `TYPE`, `type Topic` and `type Context`. `SOURCE`, `Entity`
  and `subject` come from the topic. `EnvelopeOf<E>` stays.
- **Publishing.**
  - `application/events.rs` maps the domain's `XEvent` exhaustively to the
    topic's enum.
  - It publishes with `PendingEvent::on::<UserTopic>(version, &state, event)`,
    where `state` is the topic's `Entity`. `version` is commit 4's; until then
    it's absent.
  - The domain-to-wire path then has two exhaustive matches, one on each side.
- **Subscribing.**
  - **Whole topic:** `Handle<UserTopic>` receives
    `TopicEnvelope<UserTopic> { id, key, version, time, entity, event }`, and the
    handler matches `event` with no wildcard arm. It registers one subscription
    row per `UserEventKind`.
  - **Filtered:** `Handle<UserDeleted>` and `.on::<UserDeleted>(queue, h)` mean
    "this topic, this event type only". The handler gets `EnvelopeOf<UserDeleted>`
    and needs no `unreachable!()` arms. Today's two subscribers
    (`notes.archive_deleted_authors_notes`, `users.welcome_new_user`) stay
    filtered, and their code barely changes.
  - A queue may list several types of its one topic. Queues of different topics
    stay separate.
- **3ad3e53's check** becomes "this delivery's topic and type are ones this
  queue registered".
- Update `users`, `notes` and the module template (and so `make new-module`)
  to declare their topic with the macro.

### 4. Every event carries its root's version
- `PendingEvent::on` takes the version as a required argument, so an event
  without one doesn't compile. The publisher stores it on the outbox row and in
  the envelope.
- **In the envelope it is `entityversion`**, a custom CloudEvents extension
  attribute holding an integer. It is not CloudEvents' `sequence`, which is a
  string that must sort lexicographically and has no stated scope, whereas this
  is an integer scoped to one `topic` and `partitionkey`. Record this reason in
  spec §4.
- **Test drivers return the version beside the payload.** `announced::<E>()`
  returns `Vec<(Version, PayloadOf<E>)>`, so a behaviour asserts
  `[(Version::from_i64(2), announced(&note))]`. It doesn't return the whole
  envelope: the envelope's `id` is random, so it can't be compared whole
  against a seed, and its `time` is already fixed by the `FixedClock`. Change
  `notes`' driver first, then copy it to `users` and the template.
- **What each kind of event carries:**
  - A save publishes the `Version` that `save` returns.
  - A **deletion publishes the version it removed plus one**, `Version::next()`.
    That makes it strictly the newest event for its key; with the deleted version
    it would tie with the last update. `users`' `delete` announces
    `expected.next()`.
  - `notes`' `archive_notes_by` publishes each note's new version.
- Add `topic`, `key` and `version` to `messaging.outbox`, with a unique index on
  `(topic, key, version)`. The index enforces the existing "one event per root
  per save" rule: a second event for the same save fails it.
- Fix the `time` doc comment, and spec §4's "the time it happened", to say
  what it is: when the publishing transaction recorded it, by that instance's
  clock. It doesn't order events; `version` does.

### 5. In order per key, per subscription
- Add `key` and `version` to `messaging.job`, copied from the outbox by the
  relay.
- **One relay at a time.**
  - The relay takes `pg_try_advisory_xact_lock(<constant>)`. If another worker
    holds it, the relay returns 0.
  - It reads the batch `ORDER BY seq` without `SKIP LOCKED`, then marks it and
    fans it out in one transaction:
    1. select the batch;
    2. `UPDATE outbox SET dispatched_at`;
    3. `INSERT INTO job … SELECT … JOIN subscription`.
  - Together with "commit order is version order per key", this guarantees a
    key's v3 job exists (or was handled, or was filtered out) before its v4 job
    does.
  - Workers still claim jobs in parallel.
- **The claim waits for earlier versions of the same key in the same queue:**
  ```sql
  AND NOT EXISTS (
    SELECT 1 FROM messaging.job e
    WHERE e.queue = j.queue AND e.key = j.key
      AND e.version < j.version AND e.dead_at IS NULL)
  ```
  A job's row exists until the transaction that handles it commits its deletion.
  So while v3 runs, backs off or waits for a worker that can decode it, v4 can't
  be claimed. Index: `(queue, key, version) WHERE dead_at IS NULL`.
- **Semantics** (write them in spec §4 and D9, replacing "nothing is ordered"):
  - **In order per key, per subscription.**
  - **No order across keys, topics or subscriptions.** For example, `UserDeleted`
    and a racing `NoteCreated` by that user stay unordered, as D9 already
    accepts.
  - **Gaps are normal.** A filtered subscription, or one registered after some of
    a key's events, sees only some versions.
  - **A failing job holds up only its own key, in its own queue.** It holds it up
    until it succeeds, or until it's dead-lettered, which takes about 2.5 minutes
    across 5 attempts. After that the key moves on and the dead job is skipped.
    For a state copy, skipping is harmless: the next event carries the whole
    state.
  - **Rerunning a dead letter after later versions have run delivers it out of
    order.** Say so in `DeadLetter`'s doc. This is why the consumer rule below
    still applies.
- **Consumer rule** (CLAUDE.md "Side effects and reactions", and spec §4):
  - A subscriber that keeps a copy of state stores the version with it.
  - It applies an event in one conditional statement:
    `INSERT … ON CONFLICT (id) DO UPDATE … WHERE copy.version < EXCLUDED.version`.
  - It keeps a tombstone row for a deletion.
  - This is a backstop, needed only for reruns of dead letters and for a lease
    that expires mid-run. Ordered delivery is the main mechanism.
  - Reactions to facts (mail, `notes`' archiving) don't compare versions.

### Migrating `messaging`
- The new columns arrive in new migrations, never by editing `0001`.
- `messaging.outbox` and `messaging.job` can't be backfilled with keys and
  versions. So the migration that adds them:
  - refuses to run while any undispatched outbox row or undead job exists
    (deploy step: drain the worker first);
  - deletes the dispatched outbox rows from before topics.

  Say this in the migration's comment and in the release note.

## Acceptance criteria

### Deploy safety
These tests go in `crates/common/db/tests/`, using two workers on one database
built from different `Handlers`, plus small test topics.
- [x] An old worker never claims a job of a `(queue, event_type)` it doesn't
      serve; the new worker runs it. Assert no attempt and no `last_error` on
      that job. Fails on `main`.
- [x] After registration, a relay run by the old worker still fans out to the
      new subscription. Fails on `main`.
- [x] An event relayed before a subscription is registered gets no job for it.
- [x] Registering a queue under a second topic fails.
- [x] Re-registering a retired queue fails, and retiring dead-letters the
      queue's pending jobs.
- [x] A worker whose subscriptions aren't registered refuses to start.

### Order
- [x] One key with v1, v2, v3, where v2 fails once: v3 doesn't run until v2
      succeeds.
- [x] When v2 is dead-lettered, v3 runs.
- [x] A second key proceeds while the first is held up.
- [x] A whole-topic queue receives two event types for one key in version order.
- [x] Two relays running at once never invert a key's versions, and never fan
      one event out twice.
- [x] A job whose type this worker can't decode holds up later jobs of its key
      in that queue, and no others.

### Contract
- [x] Every published event carries its root's version, stored on the outbox
      row and in the envelope.
- [x] The `users`/`notes`/template drivers' `announced::<E>()` returns
      `(Version, PayloadOf<E>)`, and every behaviour asserts the version,
      including `UserDeleted` carrying the deleted version plus one.
- [x] `notes`' `archive_notes_by` publishes each note's new version.
- [x] A second event for one root in one save is refused by the outbox's
      unique index.
- [x] Each topic's enum is exhaustive, and its type strings are unique (the
      generated test).
- [x] Every topic round-trips through `encode` and `decode`.

### Docs and checks
- [x] `EventEnvelope::time` is documented accurately.
- [x] Spec §4 and D9 describe:
  - topics, keys and versions;
  - claim filtering;
  - subscriptions registered at migrate time, and their start-from-registration
    semantics;
  - retiring a subscription;
  - order per key and its limits;
  - the consumer rule.
- [x] CLAUDE.md "Side effects and reactions" follows the new contract.
- [x] `crates/server/tests/composed_reactions.rs` passes.
- [x] `make check` passes, and `make prepare` has been run for the changed
      `query!`s.

## Out of scope
- **Starting a new subscription from history.** Kafka's `earliest` (copy the
  topic's retained outbox rows into the new queue at registration) and
  `compacted` (only the newest version per key, the cheap way to bootstrap a
  state copy). Both fit this design, and are follow-ups. `compacted` overlaps
  with published views (D8), so it needs its own decision.
- **Pruning the outbox.** Without `earliest`, nothing reads dispatched rows
  except tests and diagnostics.
- **Order across keys or topics.**
- **Catch-all variants for consumers that rebuild separately.** This is only
  needed if a module is lifted out of the monolith (D1).
- **Lowering `APP_WORKER_POLL_MS`, and `LISTEN`/`NOTIFY`.**
  - `NOTIFY` takes a database-wide lock at commit, and doesn't work behind
    transaction-mode poolers.
  - Tuning the poll interval is a separate one-line config change.
- **Offsets over the outbox.** They need visibility checks based on transaction
  ids (`pg_snapshot_xmin`), because `seq` order isn't commit order. Ordering per
  key doesn't need them.

## Notes
- **No module subscribes to a whole topic yet.** Cover that path with the test
  topics in `crates/common/db/tests/`, and don't invent a production consumer
  to exercise it.
- **Where the tests go.** Worker tests belong in `crates/common/db/tests/`, which
  today holds only `map_db_error.rs`, as `#[sqlx::test(migrations = false)]`.
  Build the test events as the unit tests in
  `crates/common/persistence/src/messaging.rs` do.
- **The invariant ordering depends on.** Every write to an aggregate, including
  a change to a child, saves the root with a version guard. If a future module
  writes a child row without bumping its root, its events are no longer ordered.
  State this next to the `Repository` rules in §4.
- Read CLAUDE.md "Side effects and reactions" and spec §4 and D9 first. Copy
  `notes` for driver changes.
