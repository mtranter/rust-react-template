# A Capability owns its Modules' data, its App shows what each user may use, and data appears across Apps without changing who owns it

Status: proposed (ticket 0074). Amends [ADR 0040](0040-code-declares-what-can-be-granted-admins-grant-it-at-runtime-and-lists-filter-in-sql.md): Modules have no gate, and resource grants alone guard operations. Also amends [ADR 0046](0046-the-spa-is-a-suite-of-apps-in-one-shell-each-with-a-few-areas.md): its App is a Capability's, and each Area declares what it needs.

The suite grows products (a CRM, timesheets, billing, rostering), each built from several Modules: a CRM might be contacts, deals and a projection searching both. Two gaps:
- **Nothing in the code says which Modules make a product.** ADR 0046 gave the SPA an App per product, but only as a UI grouping, and the glossary let one Module serve several Apps. `crates/modules/` was flat.
- **Admins grant by Module, not by what people do.** Every Module had a gate (`module:<name>` / `use`) checked before every operation, as well as grants on its resources. An admin therefore granted twice, and could open half a product by mistake.

This template is the base of a software factory for people who aren't engineers. They think in products, screens and data, not in Modules or coupling. They expect a person to show up in HR, Performance and Resourcing alike. So "which App does this belong to?" is really two questions, and they want different answers.

**Ownership is strict, one-to-many, and in code.** A Capability *keeps* data: it is where that data is written, changed and answered for. Every Module belongs to exactly one Capability. A non-engineer is asked about ownership only as "where do you go to change it?", and the factory turns the answer into a folder.

**Presence is loose, many-to-many, and on screens.** Any App may show data that another Capability owns. How it does so is below.

**A Capability is one product of the suite, and the code's name for it.** It owns a group of Modules and has one App in the SPA: an entry in the shell's registry, with its Areas (ADR 0046). Customers may be shown another word ("App", "Capability", a product name): that word is the registry's `label` and the Catalog's, never an identifier. The App is first-class and the same for everyone. Which of its Areas a user is shown depends on what they may do, as below; which Apps exist never does.

**A Capability is a folder.**
- App Modules live in `crates/capabilities/<capability>/<module>/{core,api}`, beside a `capability.yaml` that holds the Capability's id and its label for admins.
- A Module belongs to the Capability whose folder it sits in. `make check-modules` fails when the SPA's registry disagrees.
- The platform's Modules stay in `crates/platform/<module>/`, each its own Capability (`access`, `assistant`).
- The reference Modules stay in `crates/reference/`, one Capability, `notes`.

**A Capability is not a crate and owns no code.** It has no shared crate, no `-api` of its own and no tables. Its Modules meet only through their `-api` crates and never share a schema, whether or not they share a Capability (ADR 0008, 0009, 0038). "Same Capability" must not become a licence to couple. Spaghetti comes from two places writing one fact, never from one fact shown in many places.

**No gates: resource grants alone guard operations.** `module:<name>` / `use` is gone, and no `capability:` gate replaces it.
- An operation needs its action on its resource, granted to one of the caller's roles, within a Scope (ADR 0040, unchanged otherwise).
- An admin grants in the terms the business uses ("Sales can list contacts", "Sales can view skills"), once.
- **The safety net goes with the gate.** A Module's default grants to `user` must be exactly what every signed-in user may do. Nothing outside them fences the Module any more.

**An Area declares what it needs; what a user is shown follows.**
- Each Area in the registry names the action it needs on a resource (Contacts needs `contact` / `list`).
- A user is shown an Area when one of their roles is granted that action, at any Scope.
- They are shown an App, in the launcher, switcher and Go to, when it has at least one Area they are shown.
- `access` answers the caller's granted actions in one new operation, `get_my_permissions`: each resource and action allowed to them at any Scope, from the compiled rules. It replaces `useManagesAccess` and every per-App hook. The SPA reads no roles from its token.
- Hiding is a convenience, never the protection: the server refuses the operations themselves.

**Data appears in another App in one of three ways, the first that fits.**
1. **Link** to the owner's App ("View in Resourcing"). No coupling.
2. **Embed** the owner's panel in a slot on the other App's page. The panel calls its owner's API with the user's token, so the owner's grants decide what it shows, and the backend has no coupling. The shell wires the slot, as it wires the registry: one feature never imports another. Not built yet (ticket 0076).
3. **Copy**: the other App's Module enriches from, or projects, the owner's `-api` or Topic, when it must sort, filter or search by that data (ADR 0038). The owner decides what it publishes; the copy is then guarded by the copying Module's grants. So nothing the owner wouldn't show to everyone who may see the copy is ever published.

**Admins grant from Capabilities.** The Catalog lists each Capability, then its Modules' resources under it. The access screen reads as "what can Sales do in the CRM?".

**Grants already made keep their effect.** Once, `migrate` does two things:
- It revokes every grant on a Module's resources held by a role that wasn't also granted that Module's gate. Under the gate those grants allowed nothing, and without it they would start to.
- It then revokes every `module:*` grant.

Each revocation is announced, so no role gains or loses anything it could actually do. The Module of each resource is known in code, not SQL, so this is a step of `migrate`'s defaults, not a migration file.

**Later, not built now: a Capability can be disabled, then removed** (ticket 0075, P2). This is lower priority and nothing here depends on it; it is recorded so the design leaves room for it.
- **Disabling** happens at runtime: a `disabled` flag in the composition. The Capability stays compiled and in the spec, so its SPA code still type-checks. The server doesn't mount its endpoints, MCP leaves out its tools, and `get_my_permissions` leaves out its resources, so the shell hides its App. The relay skips its subscriptions, which are paused, not retired. Its pending deferred jobs are cancelled. Its retention Schedules keep running, so its personal data is still purged on time. Grants on its resources stay stored and go quiet: `access` already skips a grant naming what no loaded policy has. It can't be disabled while an enabled Capability depends on its `-api`.
- **Re-enabling** resumes its reactions from then, never running the backlog, and rebuilds its projections as a new Generation from `earliest`.
- **Removing** deletes the code, retires its subscriptions by migration, and revokes its grants.

## Considered options

- **Ownership: one Capability per Module** (chosen) **or many.** Many-to-many reads more naturally to a non-engineer, but it is ownership they would be choosing, not presence. Two Capabilities claiming one Module blurs who answers for its data and invites both products' changes into one crate. Presence across Apps gives them what they actually want.
- **Who sees an App:**
  - **A. A gate grant per Capability**, checked on every operation. Simple and safe by default. But data shown in another App would need the owner's whole Capability granted ("why must Sales have Resourcing to see skills?"), and admins grant twice.
  - **C. A gate grant for navigation only.** Keeps an explicit "Sales has the CRM", but it can disagree with the data grants: an App with nothing in it, or data without its App.
  - **B. Derived from each Area's declared action** (chosen). One kind of grant. It can't disagree with itself, and it hides Areas as well as Apps. It costs the gate's safety net (default grants must be exact) and one more declaration per Area.
- **Capability as metadata on a flat `crates/modules/`** (`[package.metadata.module] capability = "crm"`). Cheaper to move to, but the tree no longer shows the products, and the metadata could drift from the folder a reader trusts.
- **`crates/<capability>/<module>`**, with no `capabilities/` level. Shorter, but Capabilities would sit among `common`, `platform`, `reference` and `server`, and could collide with them by name.
- **A crate per Capability** for code its Modules share. Rejected: it becomes the crate every feature in the product edits, the coupling ADR 0038 avoids.
- **Apps as configurable bundles over Capabilities**, so users can re-bundle without moving code. Rejected for now: another concept to learn, and Link, Embed and Copy cover most of the need. Revisit when someone asks to re-bundle.

## Consequences

- An admin grants actions on resources, grouped by Capability, and never a gate. The App appears for whoever may use one of its Areas.
- Every default grant is an exact statement of what its role may do. A review of a Module's `default_grants.yaml` asks "should every user be able to do this, everywhere it shows?".
- A new Area names its required action, and `make check-modules` fails on one naming an action no Module declares.
- `make new-module CAPABILITY=<capability> NAME=<module>` creates the Capability's folder and `capability.yaml` when it is the first. The SPA's registry entry is keyed by the same id. `make check-modules` fails on a Capability with no entry, or an entry with no Capability, apart from the platform's that have no App (`assistant`).
- A Cargo workspace glob that matches nothing fails, and an app fresh from `make init` has no Capabilities. So the workspace lists `crates/capabilities/*/*/core` and `/api` only once one exists: `new-module` adds them and `init` removes them.
- Glossary:
  - **App** is a Capability's.
  - **Area** gains its required action.
  - **Gate** is removed.
  - **Capability** is defined.
  - "Capability trait" (a Module API's port) becomes "Module API trait", so the word means one thing. The ADRs that used it before this one stay as written.
- Moving a Module between Capabilities moves its folder and its registry entries. Its grants keep working, since they name resources, not Capabilities.
