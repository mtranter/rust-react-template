# Cross-module reads join Published Views, in a Read Module

When a read combines data owned by several Modules (filter, sort or page notes by their author's name), the join happens in Postgres, not in application code. Each provider publishes what others may read as versioned Published Views in a schema of its own, `<schema>_api` (`users_api.user_v1`, `notes_api.note_v1`), created by its own migrations: only the columns it is willing to support, never its tables. A Published View is a data product, a versioned read-only dataset for querying, where a Module API trait is a capability shaped by one consumer's use case. A view only gains columns (`CREATE OR REPLACE VIEW` may append them); a breaking change ships as `_v2` beside `_v1` until every reader has moved, and its Published Record moves with it (ADR 0008).

A Read Module (`note-search`) does the joining. It owns no source data and has no Domain Layer: a Web Layer, an Application Layer with a Finder trait returning Read Models, and a DB Layer with the SQL. It reads other Modules' data only through views, even for one row. It declares every view it reads in `[package.metadata.module] reads`. It has no production dependency on its providers; its tests run the providers' real migrations and Seeds (a test-only dev-dependency on their cores, the one core-to-core edge) instead of stubbing, because the views are what is under test, and assert against the providers' `-api` Seed records.

Joins, sorting and paging across Modules stay one indexed query, always consistent, with no projection to rebuild and no lag, while each provider keeps its tables private and its published surface small, named and versioned. Keeping the join out of `notes` keeps the write Modules' tests and schemas free of each other.

## Considered options

- **Composing in application code** (ids from `users-api`, then `WHERE author_id = ANY(..)`). Fine for a filter, but can't sort or page by the other Module's columns.
- **A BFF calling each Module.** The same limitation, plus N+1 calls.
- **Event-carried state transfer into a projection.** The right tool once Modules are separate services, but a lot of machinery and eventual consistency for a monolith sharing one database.
- **Letting `notes` join `users_api` itself.** Works, but couples the write Module's tests and migrations to `users`'.
- **Creating the views from the `-api` crate's own migrations.** A view depends on the tables at a given version, and two migrators per Module have independent sequences, so nothing orders them; and `-api` would need sqlx.

## Consequences

- Providers' migrations must keep their views compiling: Postgres refuses to drop or retype a column a view uses, the intended tripwire.
- A Read Module and its providers share one database, so lifting a provider out (ADR 0001) means replacing its views with a replicated copy first.
- `make check-sql` refuses any `<schema>.<object>` in a Module's SQL that is neither its own schema, its own `<schema>_api`, nor a declared view. It is a grep, not a parser.
- A Read Module that owns no data has no migrations and no Seed. One may later own derived data (a search index, a projection built from the views it reads), with its own schema like any Module.
