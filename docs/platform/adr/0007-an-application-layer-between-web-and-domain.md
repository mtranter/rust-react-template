# An Application Layer sits between Web and Domain

Each write Module has four Layers: Web, Application, Domain and DB. A handler builds a Command and calls an application Service; the Service loads an Aggregate from a Repository, runs a domain action with the Command, and saves the Aggregate in a Unit of Work. The Repository trait belongs to the Application Layer and speaks in Aggregates, never rows. The Domain is pure and synchronous and returns its own error enum: no repositories, no async, no clock, no id minting. Constructors take the id and `now`, so one code path serves the Service (fresh values) and Seeds (well-known values, ADR 0028), and `restore(..)` rebuilds persisted state.

This is built for Modules that grow: business rules live on the Aggregate, in one place, unit-testable without async or a database; orchestration and rules spanning Aggregates (email uniqueness, not-found, fresh ids, the time) live in the Service; handlers only translate. Because the Repository returns Aggregates, invariants can't be skipped by writing fields directly.

## Considered options

- **Free domain functions taking `&impl Repository`** (the original shape). Fine for CRUD, but logic and I/O interleave and invariants have no single home.
- **A Service generic over its Repository.** A trait object (`Arc<dyn XRepository>`) keeps the Service's type, and every signature naming it, free of the Repository's.

## Consequences

- More types per feature (Command, domain action, Service method, Repository method), and ceremony that is overkill for the example Modules.
- Load → act → save has a lost-update window, closed by versions (ADR 0016).
- A Layer is a folder re-exported from its `mod.rs`, so the crate names a Layer (`crate::domain::User`), never a file in it, and a Layer grows by adding files by concern.
- A port to an external system other than the database is a trait in the Application Layer, with its adapter in a folder named for the system (`users`' `Mailer` and `mail/`), as `db/` is for the Repository.
- What an Aggregate is, and how many a Module has, is ADR 0030's.
