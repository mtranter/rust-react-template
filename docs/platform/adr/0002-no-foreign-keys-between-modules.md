# No foreign keys between Modules

A Module's tables never reference another Module's tables or Published Views. A cross-module id is a plain `uuid` column. Where the business rules need it to exist, the writing Module checks it through the provider's Module API (ADR 0008) when it writes. References may dangle: a user is removed, and every consumer tolerates it (a note-search result without an author name).

## Considered options

- **Foreign keys across schemas.** They would couple migration order, deletes and locks across Modules, and stop one being lifted out (ADR 0001).

## Consequences

- `make check-sql` refuses a foreign key across schemas, as it refuses any cross-schema reference that isn't a listed Published View (ADR 0010).
- Cleaning up after a deletion is the owning Module's reaction to an Event (`notes` archives a deleted user's notes), so it is eventually consistent: a note created by a user deleted at that moment can escape it.
