# A Principal's roles are `user` plus its subject's assignments in `access`, not the token's claim

Amends [ADR 0040](0040-code-declares-what-can-be-granted-admins-grant-it-at-runtime-and-lists-filter-in-sql.md), which took a user's roles from the token's `roles` claim. Many identity providers don't issue that claim, or nest it under another name, so on them every signed-in user was refused every guarded operation, and who held which role could only be changed in the provider. No version of the app was deployed, so nothing depended on the claim.

**Roles are held in `access`, beside grants.** `access` stores roles (ticket 0031) and **assignments**: a role assigned to a subject, the token's `sub`, whether or not that subject has signed in yet. A user Principal's roles are `user`, which every signed-in user holds and which is never assigned, plus the roles assigned to its subject. The token's `roles` claim is not read: `Claims` has no `roles`, and `JwtVerifier::issue` takes none.

**The snapshot holds them, so no request reads them.** Assignments share the revision grants and roles bump, and the 5-second rebuild (`access::Rules`): the compiled `RuleSet` carries each subject's assigned roles beside its rules, and is swapped whole. The default `PrincipalResolver` is the `Authorizer` itself, reading the rule set in force; with none in request data the checker uses the `Authorizer` there. A change of assignment takes effect on every instance within the same bound as a grant. An app may still put its own `Arc<dyn PrincipalResolver>` in request data.

**Admins assign and unassign over the API.** `assign_role` and `unassign_role` (the latter with `If-Match`, as revoking a grant) are `admin`'s by `access`' baseline rules, each change bumping the revision in its unit of work and announced on `access.assignment.v1` (`RoleAssigned`, `RoleUnassigned`). `user` can't be assigned or unassigned (`400`), an assignment names a role that exists (`400`, naming `role`), and a role an assignment names can't be deleted (`409`).

**`APP_BOOTSTRAP_ADMINS` gives a fresh app its first admin, and admins can't lock themselves out.** It lists subjects that hold `admin` whatever is assigned, held by the `Authorizer` for the process's life; their `admin` can't be unassigned (`409`). While it is empty, the last stored `admin` assignment can't be unassigned (`409`). With ADR 0040's baseline rules for `access`, no grant, revocation or unassignment leaves the app without an admin.

**Seeds assign through the Module API.** `access_api::RoleAssignments` assigns a role as `Principal::System`; a module's seed, which is given the `Context`, gives its seeded users their roles through it (`users::module(access::assignments)`). A module's own tests, which have no `access`, assign in memory (`Authorizer::assign_in_memory`), as they compile default grants in memory.

## Considered options

- **Keep reading the claim, merging it with assignments.** Two sources of truth for one role, and a provider's misnamed claim would still silently grant nothing; out of scope for the spec that asked for this.
- **A resolver that reads assignments from the database per request.** Simple, but a query on every guarded request, and a change would apply at once on one instance and not the others, unlike grants.
- **Bootstrap admins as stored assignments** that `migrate` writes. A subject removed from the setting would keep `admin`, and admins could unassign the operator's way back in.

## Consequences

- Who holds which role is managed in the app, not in the identity provider; a token from any OIDC provider works as long as its `sub` is stable.
- A test, journey or seed gives a user a role by assigning it through `access` (or in memory, in a module's own tests), never in a token.
- A newly assigned or unassigned role applies within 5 seconds on each instance, as a grant does.
- A fresh deployment needs `APP_BOOTSTRAP_ADMINS`, or a seed assigning `admin`, before anyone can administer it.
