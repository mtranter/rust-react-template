# Modules keep their own copy of roots, fed from topics from the earliest

Amends [ADR 0010](0010-cross-module-reads-join-published-views-in-a-read-module.md): a Read Module no longer joins its providers' Published Views; it keeps its own copy of their roots, a **Projection**, fed from their Topics. Amends [ADR 0009](0009-the-cross-module-mechanism-follows-why-the-data-is-needed.md): a list by another Module's columns no longer needs a Read Module, since any Module may keep a Projection. Builds on [ADR 0033](0033-the-outbox-is-a-compacted-log.md) and [ADR 0035](0035-the-worker-copies-an-earliest-backfill-in-chunks.md), which made an `earliest` subscription cheap to add at any size.

ADR 0010 joined the providers' views in one query: always consistent, no lag. But it ties the Read Module to the providers' database. A provider lifted into its own service takes its views with it, and its readers break. The query can only use the indexes the provider chose for its own tables, and a search shape the provider doesn't want (trigram, `tsvector`, a denormalised author name) has nowhere to live. Every Topic already carries each root's whole state (ADR 0012), and the compacted Outbox holds every key's latest one, so a reader can build the copy it needs from the events alone, however long after the provider shipped.

**A Projection is a set of tables a Module owns, filled by `earliest` subscriptions.** `Handlers::project::<T>(queue, rows)` (`app_db::Projecting`) subscribes to the whole Topic from `earliest`. The module writes only its rows: `put` (an unguarded upsert) and `remove`, implementing `app_db::Project<T>` in its DB Layer, and says which event `removes` a key.

**The framework owns versions and tombstones.** Before handing an event to the rows, the projection records its version in `messaging.projection_version` (by queue and key), but only if it is newer than the last one applied. An older one, such as a rerun dead letter, changes nothing. A removed key keeps its version row as a tombstone, so an older event can't bring the row back. The module's own tables carry no version column and no `deleted_at`, so its queries need no filters for them.

**Any Module may keep a Projection; it is not a kind of Module.** A write Module keeps one of another Module's Topic to list or search by that Module's columns. It may keep one of its own Topic for a read shape its Aggregate's tables can't serve well (a search index, a denormalised summary), accepting that its own writes show there only once the worker delivers them; its other reads stay on its own tables. A Read Module is a Module whose only data is Projections, for a read no one Module owns: `note-search` (notes with their authors' names) stays one.

**A copy is served only once it has caught up.** `app_db::caught_up(pool, queues)` holds once each queue is registered, its backfill is copied, and every job copied by then is handled (or dead). `app_db::Generations` lists a module's generations of tables, newest first, each with its queues. A finder asks which to `serve`: the newest that has caught up. While none has, it is `unavailable` (503, `ErrorCode::Unavailable`). Once one is served, only newer ones are checked again.

**A new shape is a new generation.** Rebuilding a copy (a new column, a new index) means new tables fed by new `_v2` queues, listed first in `Generations`. The finder switches to them once they catch up, with no deploy, and serves the old generation until then. A later change retires the old queues (the ADR 0014 migration, plus `DELETE FROM messaging.projection_version WHERE queue = …`) and drops their tables.

**Seeds publish their events.** A seed saves through the Repository, as before, and now publishes the Aggregate's creation event in the same Unit of Work, as of its creation time. Otherwise a Projection would never hold seeded data. A test Template relays the seeds' events before the Driver registers its Subscriptions, as in an environment seeded before the build. Reactions starting from `latest` (welcome mail) then never see them, and an `earliest` backfill does. Drivers' `announced` leave them out (`Worker::outbox_end`, `published_after`). `make dev` seeds after `migrate`, so locally the seeded users are welcomed like new ones.

**The module template keeps one.** `widgets` keeps `widgets.widget_search` from its own Topic, fed by `widgets.search` and served by `search_widgets`, while its list stays on `widgets.widget`. `make new-module` scaffolds it into every module, to keep, point at another Module's Topic, or delete. There is no separate projection template or scaffold.

`note-search` is the reference: `note_search.note` and `note_search.author`, fed by `note_search.copy_notes` and `note_search.copy_authors`. To keep it whole, the notes entity gained `createdAt` (absent from older events, which fall back to the event's time).

## Considered options

- **Keep joining Published Views** (ADR 0010). Consistent and simple, but it can't survive a provider being lifted out, and it can't shape indexes for the reader. Views stay, for SQL readers that need one row now; no Read Module uses them any more.
- **The version as a column of each projected table**, guarded by `ON CONFLICT … WHERE copy.version < EXCLUDED.version`, with a `deleted_at` tombstone. Every module would repeat the guard, every query would filter tombstones, and the scaffold couldn't own it.
- **Serving a copy before it catches up.** Partial results look like missing data, and the reader can't tell. A 503 tells it to retry.
- **Holding readiness until the backfill completes only** (`completed_at`). That means copied into jobs, not handled, so the copy could still be missing most keys.
- **Projections only in Read Modules.** Every list by another Module's columns would need a new Module, even when one write Module is the only reader, and a Module could never keep a read shape of its own data.
- **A separate projection template and `make new-projection`.** It made a Projection look like a kind of Module, and was a second template every scaffold script had to know about (it broke the module scaffold once).
- **Seeds publishing only in `make dev`.** Tests and dev would differ, and a Projection's tests would publish seed events by hand anyway.

## Consequences

- A Projection is eventually consistent: a write shows in search once the worker delivers it, even in the Module that made it. Journeys run the worker before searching.
- A Module keeping a copy of another's roots depends on that provider's `-api` crate for its Topic. It needs no core dev-dependency: its Driver publishes the provider's events, as a Subscriber's does.
- A fresh deployment answers 503 for a read served from a Projection until its worker has run once.
- `messaging.projection_version` grows by one row per key per queue and keeps tombstones forever, as the Outbox keeps each key's latest event.
