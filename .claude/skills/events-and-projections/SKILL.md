---
name: events-and-projections
description: Publish a domain event, declare or change a topic, subscribe to or react to another module's events, build or reshape a projection or read module, retire a subscription or queue, or broadcast between server instances over the bus. Use before touching `topic!`, `application/events.rs`, `handlers(..)`, `Handlers::on`/`on_from_earliest`/`project`, `Generations`, `messaging.retire`, or `app_bus`.
---

# Events, subscribers and projections (ADR 0011–0014, 0033, 0035, 0036, 0039)

Extends `crates/AGENTS.md`, whose "Events" rules hold everywhere; this is the how. If this file and the code disagree, the code wins: fix this file in the same change.

Built on Kafka's model: a topic per root, keyed by the root's id, ordered per key by its version, with a subscription as a consumer group, hand-rolled on a Postgres outbox.

| Doing | Copy |
|---|---|
| Publishing a root's events, its topic | `crates/reference/notes`' `application/events.rs`, `crates/reference/notes/api/src/events.rs` |
| Reacting to another module's events | `crates/reference/notes`' `application/reactions.rs`, its `handlers(..)`, its driver's `user_deleted(..)` / `run_pending_events()` |
| A projection of a module's own topic | the module template's search copy (`widgets`: `db/search.rs`, `handlers(..)`, `search_widgets`) |
| A read module over several topics | `crates/reference/note-search`: `db/projections.rs`, `handlers(..)` (`Handlers::project`), `db/finder.rs` (`Generations`), its driver publishing the providers' events |

## Publishing

- **Every state change publishes one event per root, in the unit of work that saves it**, so it goes out if and only if the write commits: each use case that saves a root calls `self.publisher.publish(&mut tx, events::pending(&x, version, event))` with the `Tx` it saved in. Never call an external system or another module after `commit`: anything else that must follow a write (mail, another module's reaction) is a subscriber.
- **The domain decides what happened**: each transition returns a variant of the aggregate's plain `#[must_use]` `XEvent` in `domain/` (constructors `(Self, XEvent)`, mutations `XEvent`, `Option<XEvent>` where they may change nothing, deletion `x.delete() -> XEvent`), a past-tense fact, never a command in disguise (`UserRegistered`, not `SendWelcomeEmail`). `application/events.rs` maps it exhaustively to the topic's enum, the one place domain names meet the wire.
- **Each root has a topic**, declared with `app_events::topic!` in the `-api`'s `events::v1`: name (`<module>.<root>.v1`), `source`, `retention`, entity, key, one line per event (`NoteDeleted => Deleted: "<module>.note_deleted"`, `NoteItemAdded => ItemAdded(OfItem): ".."` for one with a context). Its event enum is deliberately exhaustive, not `#[non_exhaustive]`: a new event breaks every whole-topic consumer in the same change. Events and the entity only gain fields, each `#[serde(default)]`; removing an event or changing the entity incompatibly is a new topic (`.v2`) published beside the old. Its module declares it publishes it (`Module::new(..).publishes::<XTopic>()` in `di.rs`), so `migrate` registers it.
- **An event's payload is `EventPayload { entity, context }`.**
  - **`entity` is the root's latest state** (event-carried state transfer), so subscribers never call back; a deletion carries the state as it was last. The entity is an API, not a copy of the aggregate: its type lives in the `-api`'s `events::v1`, apart from any published view record, built in `application/events.rs`. Usually that is every property but the audit ones; leave out what consumers shouldn't depend on or see, and reshape a field where the domain's own form would leak (a secret, an internal code). Each published field is coupling you must keep.
  - **`context` says what changed where the state can't.** An event about a child names it: `NoteItemRemoved` carries the Note as its entity and `OfItem { item_id }` as its context, since from the latest state alone a consumer can't tell which item went. Use none where the entity says it all (`NoteCreated`).
  - It carries the root's new version: `PendingEvent::on::<XTopic>(version, &state, event)` takes the `Version` `save` returned, or for a deletion the one removed `.next()`, so it is the newest of its key.

## The outbox is a compacted log (ADR 0033)

Each topic declares its `retention:` in `topic!`, with no default, and `migrate` registers it in `messaging.topic`; an unregistered topic is never compacted. Compaction (`Worker::compact`) deletes a relayed event once it is older than its topic's retention, a later version of its key exists and no unfinished backfill has yet to copy it: every key's latest event stays forever, a deletion's included, and jobs are copies it never touches. So a consumer can't rely on the outbox for history older than the retention.

Compaction is not module work: it is the worker's own maintenance of the log, not a Schedule. Each worker compacts after its first poll, then after its first poll once `APP_OUTBOX_COMPACT_EVERY` has passed by its clock (`Worker::compact_if_due`), uncoordinated: N replicas compact up to N times a period, which the database alone makes safe, so add no lock, lease or schedule row.

## Subscribing

- **Subscribers are idempotent.** A subscriber implements `app_persistence::Handle<E>` for one event (receiving `EnvelopeOf<E>`) or `Handle<XTopic>` for a whole topic (matching its `event` with no wildcard arm) in its `application/`, registered in its `handlers(..)` under a stable queue name (`Handlers::new().on::<UserDeleted>("<module>.archive_deleted_authors_notes", service)` in `crates/reference/notes`).
- Handlers run in the worker inside their own `Tx`, at least once, in order per key within each queue, with no order across keys, topics or queues, and gaps where a subscription is filtered or newer than the key. A failing one backs off and holds up only its key, then is dead-lettered in `messaging.job`, as is one whose worker dies on every attempt.
- Reactions act as `Principal::System`, which policies don't apply to.
- **Subscriptions are rows in `messaging.subscription`, registered by `migrate`**, never on worker start, so old and new builds relay alike during a rolling deploy (ADR 0014). A queue is bound to one topic forever and a retired name is never reused. Retire one with a migration, never by just deleting its handler. `messaging.retire` stops relaying to the queue, completes its unfinished backfill and dead-letters what it hadn't run, serialised against the relay and any backfill chunk in flight, so neither adds a live job to the queue after it:

  ```sql
  SELECT messaging.retire('<queue>');
  -- and for a projection's queue, in a later migration once nothing serves its tables:
  DELETE FROM messaging.projection_version WHERE queue = '<queue>';
  ```

- **A subscription starts from `latest`** (`Handlers::on`): it receives the events relayed after `migrate` first registers it. A reaction to facts starts from `latest`, or it would re-run for history.
- **A copy of state starts from `earliest`** (`Handlers::on_from_earliest`), subscribing to the whole topic and comparing versions: it is backfilled once with every event its topic's outbox held when `migrate` first registered it (after compaction, each key's latest plus the history within the retention). `migrate` only records the backfill in `messaging.backfill` (complete at once when no event of its type has been relayed), and workers copy it into the queue a chunk at a time, alongside the relay (ADR 0035). Until it has caught up (`app_db::caught_up`: every event its topic held at registration, relayed then or not, delivered and handled, judged by outbox `seq`), a copy built from it is incomplete. A key's live events wait for its backfilled ones, but other keys' don't, so it can't assume it has seen all history before its first live event.
- A start is fixed once registered: changing it in code only makes `migrate` warn.

## Projections and read modules (ADR 0036)

- A copy of another module's state is a projection: `Handlers::new().project::<XTopic>(queue, rows)` (`app_db::Projecting`), with `rows` implementing `app_db::Project` in `db/`. The framework applies an event only when it is newer than the copy, since a rerun dead letter arrives out of order, and keeps a tombstone for a removed key in `messaging.projection_version`, so `put` is a plain upsert and `remove` a plain delete, with no version column. Reactions to facts (mail, archiving) don't compare versions.
- Projections are tables in a module's own schema, filled by `earliest` projections of topics, so the module needs nothing of a provider but its events and could outlive the provider being lifted out. Any module may keep one: of another module's topic to list or search by its columns, or of its own topic for a read shape its aggregate's tables can't serve well (the module template's `widget_search`), accepting that its own writes show there only once the worker delivers them; keep every other read on the aggregate's tables, and delete the template's copy when nothing needs it. An `XFinder` (or search) trait returns read models from it.
- **Read modules** (ADR 0010, 0036) are modules whose only data is projections, for a read no one module owns: they have no `domain/`. A read module isn't scaffolded: copy `crates/reference/note-search`. Shape the tables for the queries (denormalise, index), and take every column from the events' entities: if one is missing, the provider's entity gains it (`#[serde(default)]`, so the projection tolerates its absence in older events). A read module depends on providers' `-api` crates for their topics, never calls `-api` traits, has no seed, and its driver publishes the providers' events itself.
- **Serve a projection only through `app_db::Generations`**, which answers `unavailable` (503) until it has caught up. The finder serves the newest generation of tables that has caught up; a new shape is a new generation, new tables and `_v2` queues listed first, switched to once caught up, the old one retired later (the retire migration and a `DROP TABLE`).
- A projection's or read module's search has no `Columns` yet: it refuses any plan but `Allowed` (`crates/reference/note-search`, the template's `search`) rather than show rows the caller may not see.

## Testing

Background work runs only when a driver verb runs it (`run_pending_events()`); time moves only via `later(by)`. A subscriber's driver publishes the provider's event itself (`user_deleted(USER_ID_ALICE)` in `crates/reference/notes`), as it stubs an `-api` trait. A state change is checked to have announced its event at its version (`announced::<E>()`); a refused action, to have announced nothing. A test `TEMPLATE` relays the seeds' creation events before the driver registers its subscriptions, and `announced` leaves them out (`Worker::outbox_end`, `published_after`). If the subscriber crosses modules, add or extend a journey: each module's tests stub what's across its boundary, so only a journey notices a payload drifting from what its subscriber expects.

## Broadcasts between instances (ADR 0039)

Broadcasts go over the bus, never the outbox, whose queues each deliver a job to one worker. A module that publishes or follows a channel builds a `app_bus::Channel<T>` from `ctx.bus()` (its name prefixed by the module: `<module>_changes`), with a `Hub<T>` where the instance holds the clients (an SSE stream, a WebSocket), and registers as `Module::new(..).broadcasts()`, which makes the server and worker refuse to start without `APP_REDIS_URL`. The bus loses messages: keep the truth in the database and treat `Received::Gap` as the cue to recheck it. `T` changes only compatibly, as an event does. Tests start their Valkey with `app_bus::testing::Valkey` (the bus crate's `test` feature) and pass `ctx.with_bus(Arc::new(valkey.bus().await))`; nothing else touches Redis.
