---
id: 0004
name: Publish seed data through -api, decouple note-search, and state one cross-module rule
status: done
priority: P1
size: M
blocked_by: []
blocks: [0005]
source: architecture review 2026-10-31, finding 5
---

# Publish seed data through `-api`, decouple note-search, and state one cross-module rule

## Description
There are two cross-module mechanisms: `<module>-api` traits (D7) and published
views read by read modules (D8). Their rules are opposite, and nothing says which
to use when. The note-search read module also breaks the isolation the docs
promise:
- its seed depends on the `users` and `notes` core crates;
- it builds notes aggregates with notes' domain constructors;
- it writes module-3 ids into `notes.note`;
- its test driver builds expected values from `notes::Note` and `users::User`.

As a result, a change to `Note::create` or to a `Note` getter breaks note-search,
so an agent working in `notes` has to read note-search too.

The root cause is that seed data can only be described in a module's core crate
(as aggregates), so the only way to line data up across modules is to depend on
cores. The fix is to publish what other modules may know about seeded data
through the provider's `-api`, which consumers may already depend on.

## Evidence (as of 2026-10-31)
- **Optional core deps.** `crates/modules/note-search/core/Cargo.toml` has optional
  `[dependencies]` on `users` and `notes` (seed feature) plus dev-deps on both.
- **Seed uses notes' domain types.** `crates/modules/note-search/core/src/seed.rs`
  uses `notes::{AuthorId, CreateNote, Note, NoteId}` and `users::seed::{ALICE, BOB}`.
- **Driver uses domain types.** `tests/note_search/driver.rs` builds
  `NoteSummaryView::of(note: Note, author: Option<User>)` from aggregate getters.
- **A hook for one consumer.** `crates/modules/notes/core/src/seed.rs` exposes
  `save_missing` only for note-search.
- **Made-up author.** `notes::seed::AUTHOR` is not a user, so the dev DB has no
  notes by real users except the ones note-search's seed adds; `notes`' test stub
  `KnownUsers` recognises only that invented id.
- **No rule for choosing.** `notes` checks author existence through
  `users_api::UserDirectory`, though it could have read `users_api.user_v1`; nothing
  tells an agent which to choose.

## Decisions
1. **Cross-module rule.** The choice follows *why* the data is needed, not how
   much of it:
   - **A domain operation needs an answer.** When a module's business logic needs
     to know something about another module's data, to decide or validate, it
     calls the provider's `<module>-api` trait. That holds at any size, so bulk
     needs get batch methods (`users_exist(ids)`, implemented with
     `WHERE id = ANY($1)`).
   - **A query presents data that spans modules.** An endpoint that lists,
     searches, filters, sorts or pages by another module's columns lives in a read
     module that joins the providers' published views. Size matters here only as
     the reason a trait can't do the job.

   A published view is a **data product**: a versioned, read-only dataset a module
   publishes for querying (read modules now, perhaps reporting or exports later).
   A trait is a capability shaped by the consumer's use case. A read module reads
   other modules' data only through views, even for a single row, never calls
   `-api` traits, and never writes another module's data. A write module never
   reads views; a list it can't serve from its own schema moves to a read module.
2. **No foreign keys between modules.** A module's tables never reference another
   module's tables (or views). A cross-module id is a plain `uuid` column, its
   existence checked at write time through the provider's `-api` trait where the
   business rules need it. References can dangle (a user is removed), and every
   consumer tolerates that (note-search's `author_name: None`). **Why:** a foreign
   key would couple migration order, deletes and locks across modules, and stop a
   module being lifted out (D1). `make check-sql` already refuses one (a
   cross-schema `REFERENCES` isn't a listed view); today the rule lives only in a
   comment in `notes/core/migrations/0001_init.sql`.
3. **Published records.** Each published view has a plain Rust row type in the
   provider's `-api`, in a version module that mirrors the view's version:
   `users_api::v1::User` ↔ `users_api.user_v1` (`id, display_name`),
   `notes_api::v1::Note` ↔ `notes_api.note_v1` (`id, author_id, title, created_at`).
   Fields are only ever added; a breaking change is `v2::User` beside `v1::User`,
   as `user_v2` beside `user_v1`. Always written with the version path
   (`v1::User`), never imported bare, so it can't be confused with the aggregate.
   Records are `#[non_exhaustive]`, so adding a field can't break a consumer:
   consumers never construct them, they get them from `seed::*`. Records describe
   the data product only; `-api` traits return their own types, so a view gaining
   a column never changes a trait's signature or breaks a consumer's stub.
   Not named `*View`: drivers already use `NoteView`/`UserView` for HTTP responses.
4. **Seed data is owned by the owning module and described in its `-api`.** Under
   a `seed` feature, `<module>_api::seed` exposes well-known id `const`s (plain
   `Uuid`) and one `fn` per seeded entity returning its published record
   (`users_api::seed::alice() -> v1::User`). The core seed builds each aggregate
   from that record, adding the unpublished fields (email, body), and saves it, so
   there is one source of truth. Saving (`standard(pool)`) stays in the core:
   `-api` stays pure.
5. **Seeds may reference other modules' seed ids, only through `-api`.** A seed
   follows the dependency graph: `notes`' seed authors notes as
   `users_api::seed::{ALICE, BOB}`. This replaces "never, except a read module's".
   A seed saves only its own module's data.
6. **A module seeds only data it owns.** note-search owns none today, so it has no
   seed. A read module may later own *derived* data (embeddings, search indexes,
   denormalised projections built from the views it reads); it then has its own
   schema, migrations and seed like any module, and its seed names provider data
   only through `-api` seed ids. How such data is kept fresh is out of scope here
   (see 0007). Read-module tests run the providers' migrations and
   `seed::standard` (dev-dependencies on provider cores, the only allowed
   core→core edge, test-only) and assert against `-api` seed records.

## Work
1. **users-api.** Add `v1::User`, and a `seed` feature with `ALICE`/`BOB` and
   `alice()`/`bob()`. Depends on `app-domain` for `well_known_id` (pure).
   `users::seed` builds its aggregates from these records; `users::seed::ALICE`
   etc. become re-exports or go away in favour of the `-api` ones.
2. **notes-api (new crate).** `crates/modules/notes/api`, package `notes-api`: pure
   (no poem/sqlx), `v1::Note`, and a `seed` feature with note ids and records.
   Register it in the root workspace dependencies; it goes in neither `deny.toml`
   `wrappers` list (it's an `-api` crate).
3. **notes' seed.** Notes authored by `users_api::seed::{ALICE, BOB}`, plus one note
   whose author is deliberately not a user (`ORPHANED_NOTE` by `UNKNOWN_AUTHOR`),
   so note-search's "unknown author" case is intentional. The notes take fresh
   sequence numbers (3–5): the retired `FIRST_NOTE`/`SECOND_NOTE` ids are never
   reused, since an environment already seeded keeps their old rows. Delete `save_missing`. `notes`' driver stub `KnownUsers` recognises
   exactly the `users_api::seed` ids; update notes' tests that use `seed::AUTHOR`.
4. **note-search.** Delete `src/seed.rs`, the `seed` feature, the optional
   `app-domain`/`users`/`notes` deps, and the `with_seed` in `di.rs`. Keep the
   dev-dependencies on `users`/`notes` (migrations and `seed::standard` only) and
   add dev-deps on `users-api`/`notes-api` with `seed`. The driver builds
   `NoteSummaryView::of(note: notes_api::v1::Note, author: Option<users_api::v1::User>)`;
   tests name provider data only through `users_api::seed`/`notes_api::seed`.
5. **Registration and checks.** Remove `note-search/seed` from the server's `seed`
   feature. Teach `scripts/check-modules.sh` that a module may have no seed: key the
   `seed` feature and `pub const MODULE` checks off the presence of `src/seed.rs`,
   not off being a read module (one that later owns derived data will seed). The
   number is read from the `-api` seed if there is one, else the core's (and
   `make new-module` allocates from both). Module number 3 is released: no seed
   id is built from it any more.
6. **Docs.**
   - `CLAUDE.md`: a one-paragraph "which cross-module mechanism?" rule (Decision 1);
     "no foreign keys between modules" in the Cross-module bullets (Decision 2);
     update the Seeds bullet (Decisions 4–6).
   - `spec.md`: D1 or D7 (no cross-module foreign keys; dangling references are
     tolerated), D7 (domain operations of any size, batch methods; published
     records; seed ids through `-api`), D8 (views are data products; read modules
     read others' data only through views, even for one row; "owns no tables"
     becomes "owns no source data", leaving room for derived data; write modules
     never read views), §13 (seed rule).
   - `CONTEXT.md`: update **Seed** and **Read Module** (views only; may own derived
     data, never source data); describe a
     **Published View** as a data product; add **Published Record** (the plain Rust row of
     a Published View, in the provider's Module API, used to describe seed data
     and as consumers' assertion values).

## Acceptance criteria
- [x] `note-search`'s `[dependencies]` (all features) contain no module core crate,
      and it has no `seed.rs` or `seed` feature.
- [x] note-search's tests import nothing from `users`/`notes` core except
      `migrations()` and `seed::standard`.
- [x] Changing `notes::Note::create`'s signature, or renaming a `Note` getter,
      doesn't require touching note-search.
- [x] `notes::seed::save_missing` is gone; no seed writes another module's tables.
- [x] `users-api` and `notes-api` stay pure (cargo-deny passes) and expose
      `v1::*` records plus `seed` ids and records behind a `seed` feature.
- [x] `CLAUDE.md` has a one-paragraph "which cross-module mechanism?" rule and
      states that modules never hold foreign keys to each other.
- [x] `make dev` still shows notes with author names on `/search`, including one
      note with no author name.
- [x] `make check` passes.

## Out of scope (follow-up ticket)
- **Check the view contract against the record.** A core test that compares the
  live view's columns (`information_schema.columns`) with `v1::*`'s fields, so the
  `-api` declaration can't drift from the migration. Later, `make check-sql` could
  use these declarations instead of grepping.
- **View SQL in `-api` migrations.** Rejected: a view depends on the tables at a
  given schema version, and two migrators per module have independent version
  sequences, so nothing keeps them ordered. It would also need sqlx in `-api`.
  Views stay in the core's migrations; the record in `-api` is the visible contract.
