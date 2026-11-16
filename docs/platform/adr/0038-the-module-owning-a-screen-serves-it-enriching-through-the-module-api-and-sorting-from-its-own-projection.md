# The Module owning a screen serves it, enriching through the Module API and sorting from its own Projection

Amends [ADR 0009](0009-the-cross-module-mechanism-follows-why-the-data-is-needed.md): a Module API call may also enrich a page already read, not only decide or validate. Builds on [ADR 0036](0036-modules-keep-event-fed-projections.md).

Most roots refer to other Modules' roots: notes, jobs and anything else carry a `created_by` user. Asking for a Projection whenever a screen shows another Module's column would give every Module a copy of users. Asking for a Read Module instead would put every such screen in a few shared Modules. Both work against how the repo is built: by agents in parallel, one feature to one Module, each with the least context that is correct. A shared Module is a file every feature changes, so parallel work collides there, and an agent changing it needs every Module it serves in context.

Most of those screens only show the other Module's column. Sorting, filtering, searching or paging by it needs the rows in one query, so it needs a copy; showing it beside a page already read doesn't.

**Showing is enrichment.** A Module reads a page from its own tables, then asks the provider's Module API once, in a batch, for just that page's ids (`display_names(ids)`), tolerating ids the provider no longer has. Never per row, never a whole table, never to decide what the page holds.

**Sorting by it is a Projection in the Module that owns the screen.** It copies only the columns the read uses, even when other Modules keep their own copy of the same Topic. Copies are narrow, built from Events, and evolve independently, each a Generation of its own Module. A Read Module stays for a read no one Module owns, such as a search across several Modules' roots.

**Each Module keeps its own Web Layer.** A screen spanning Modules is an operation of the Module that owns it, served by the two mechanisms above.

## Considered options

- **One backend-for-frontend over every Module**, with Modules exposing their use cases in-process and no HTTP of their own. Composing at request time can't sort or page by another Module's columns, so Projections are still needed; it tempts a request into several Modules' commands, outside one Unit of Work; it adds a mapping layer per Module (facade DTOs, then HTTP types); every feature changes it; and screen-shaped operations make poor MCP tools (ADR 0034).
- **One shared copy of users** (a common Read Module, or a Projection crate every Module reads). One copy, but a shared schema every Module's queries couple to, changed by every feature that needs a new user column.
- **Joining a Published View of users from write Modules.** No copies and always consistent, but it ties every reader to the provider's database, which ADR 0036 left behind, and `make check-deps` forbids `reads` in a write Module.
- **A Projection for every cross-Module column, shown or sorted.** Consistent rules, but a copy, its queue, backfill and migrations for a name shown beside an id.

## Consequences

- A page shows the provider's current value, with no worker lag; a sort by it uses the copy's, so the two can briefly disagree. Accepted.
- Several Modules may keep copies of one Topic, each with its own queue and backfill. Adding a column to the provider's entity reaches each copy only when its Module adds a Generation for it.
- Providers gain batch lookup methods on their Module API traits as screens need them; a consumer's Driver stubs them, as it stubs `exists`.
