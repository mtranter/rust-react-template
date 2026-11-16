# The cross-module mechanism follows why the data is needed, not how much

Amended by [ADR 0036](0036-modules-keep-event-fed-projections.md): a list by another Module's columns is served from a Projection, which any Module may keep, not from a Read Module over views.
Amended by [ADR 0038](0038-the-module-owning-a-screen-serves-it-enriching-through-the-module-api-and-sorting-from-its-own-projection.md): a Module API call may also enrich a page already read.

There are two ways to use another Module's data, and the choice follows why it is needed. When a Module's business logic needs to know something about another Module's data, to decide or validate, it calls the provider's Module API trait (ADR 0008), at any size: a bulk need gets a batch method (`users_exist(ids)`, implemented with `WHERE id = ANY($1)`), not a view. When an endpoint presents data spanning Modules, listing, searching, filtering, sorting or paging by another Module's columns, it belongs in a Read Module over Published Views (ADR 0010).

A write Module never reads views and never joins across schemas: a list it can't serve from its own schema moves to a Read Module. A Read Module never calls Module API traits and never writes another Module's data.

## Considered options

- **Choosing by size: views for anything bulk.** Puts views, and the providers' migrations, into write Modules' validation and tests.

## Consequences

- `make check-deps` fails a write Module (one with `src/domain/`) that declares `reads`, and `make check-sql` refuses a view not declared (ADR 0017).
