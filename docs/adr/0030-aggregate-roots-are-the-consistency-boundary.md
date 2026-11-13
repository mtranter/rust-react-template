# Aggregate Roots are the consistency boundary, separate from Modules

A Module and an Aggregate answer different questions. A Module groups what changes for the same reasons. An Aggregate Root draws a consistency boundary: what must change together, in one transaction, under one version. So a Module may own several roots, and a root may own Child Entities. A Child Entity has no Repository, no Service, no Topic and no version of its own: it is loaded, changed, saved and announced only through its root. A use case saves exactly one root; effects on other roots, even in the same Module, follow by Event.

Two things go wrong otherwise. Rules that span the children leak into the Service and into database constraints. And because Events are ordered only per key (ADR 0013), Events about a parent and its children on separate Topics reach Subscribers in no particular order.

The reference is `notes`: a Note owns its Items, a checklist. Adding or removing an Item is a change to the Note, saved under the Note's version guard and announced on the note Topic (`ItemAdded`, `ItemRemoved`), with the Note as entity and a context naming the Item (`ItemRemoved(OfItem { item_id })`, marker types `NoteItemAdded` and `NoteItemRemoved`).

## Considered options

- **One aggregate per table** (the scaffold's shape, and what a first child usually gets). It makes the Service the real consistency boundary: it holds several Repositories and runs cascades itself, a rule across children lives only in a constraint, and a child's Events have no order relative to its parent's.
- **One aggregate per Module.** Rejected: two roots in one Module often share no transactional rule, and would contend for one version.
- **Versions per child.** Avoids one child's change making another stale, but needs a type beside each Aggregate holding the children's versions, and a client's version and the save's guard then differ.

## Consequences

- A root's version is the Aggregate's only one: it guards every save, orders its Events, and is the `ETag` and `If-Match` of the root and of every child. A change to one Item makes a client's copy of another stale; that contention is accepted.
- An Event about a child is published on its root's Topic, keyed by the root's id. Its entity is the whole root, children included, and its context names the child.
- A Subscriber reacting to one Event may change several roots (every note of a deleted user). Each is saved and announced as its own change, still in that delivery's Unit of Work.
- Cardinality bounds a Child Entity: the whole Aggregate is loaded for every write and carried by every Event. A child that grows without bound is a reason to revisit the boundary.
