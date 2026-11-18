---
id: 0032
name: "A Principal's roles are `user` plus its subject's assignments in `access`, not the token's `roles` claim"
status: in-review
spec: docs/platform/specs/0001-admins-assign-roles-to-users-within-the-app.md
blocked_by: [docs/platform/tickets/0031-roles-are-records-in-access-that-admins-list-creat.md]
blocks: [docs/platform/tickets/0033-access-records-who-signs-in-and-admins-manage-user.md]
---

# A Principal's roles are `user` plus its subject's assignments in `access`, not the token's `roles` claim

`access` stores assignments of roles to subjects (the token's `sub`). A user Principal's roles are now `user` plus those assignments, read from the same compiled snapshot as grants and roles, so no request reads the database for them. The token's `roles` claim is no longer read. `APP_BOOTSTRAP_ADMINS` gives a fresh app its first admin, and admins can't lock themselves out.

Scope:
- **Storage and snapshot.** `access.assignment` maps a subject to roles. It shares the revision and the 5-second rebuild with grants and roles.
- **Resolving the Principal.** A `PrincipalResolver` backed by the snapshot replaces `ClaimsResolver` as the default. `Claims` loses `roles`, `JwtVerifier::issue` stops taking roles, and `Principal::System` is unchanged.
- **Operations.** `assign_role` and `unassign_role`, with agent-facing summaries, taking `If-Match` where a change can race. `RoleAssigned` and `RoleUnassigned` go on the topic, and each change bumps the revision.
- **Validation and lockout.** `user` can't be assigned or removed (`400`). An unknown role is refused `400`, naming the field. Deleting a role that an assignment names is refused `409`. Removing the last stored `admin` assignment is refused `409` when `APP_BOOTSTRAP_ADMINS` is empty.
- **Bootstrap admins.** `APP_BOOTSTRAP_ADMINS` is a comma-separated list of subjects that always have `admin`. They can't be unassigned.
- **Migration of callers.** Every test, journey and driver that put roles in a token now assigns them through `access`. Development seeds assign their seeded users' roles through `access`'s Module API, as `System`. The Vite dev token (`frontend/vite.config.ts`, `APP_DEV_ROLES`) stops carrying roles, and dev gets its admin from seeds or `APP_BOOTSTRAP_ADMINS` instead.
- **Docs.** ADR 0040 is amended or superseded: roles come from `access`, and a resolver reading the token is no longer the default. `crates/AGENTS.md` is updated to match.

## Acceptance criteria
- [ ] A token with a `roles` claim gains nothing from it. A Principal with no assignments has exactly `user`.
- [ ] Journey: a user is refused an operation that needs `support` (`403`). An admin assigns them `support`, and within 5 seconds (driven by the test's clock or the poll interval, not a sleep) the user is allowed. After `unassign_role` they are refused again.
- [ ] Assigning or unassigning `user` is refused `400`. Assigning a role that doesn't exist is refused `400`, naming the field. Neither bumps the revision.
- [ ] Deleting a role that an assignment names is refused `409`.
- [ ] With `APP_BOOTSTRAP_ADMINS` empty, removing the last stored `admin` assignment is refused `409`. A subject listed in `APP_BOOTSTRAP_ADMINS` has `admin` with nothing assigned, and unassigning `admin` from them is refused.
- [ ] Resolving a Principal issues no database query; a test proves it.
- [ ] `Claims` has no `roles` field. No test, journey or seed puts roles in a token.
- [ ] After `make seed`, the seeded users have their roles through `access`. With `make dev`, the admin screen is reachable as the dev user without `APP_DEV_ROLES`.
- [ ] `assign_role` and `unassign_role` are in the spec and on `/mcp`, and `make check-init` still passes.
- [ ] Before this Ticket's PR is merged:
  - [ ] ADR 0040 is amended or superseded;
  - [ ] `.env.example` documents `APP_BOOTSTRAP_ADMINS`;
  - [ ] `docs/platform/glossary.md` gains **Assignment**.
- [ ] `make generate` is committed, and `make check-changed` passes.
