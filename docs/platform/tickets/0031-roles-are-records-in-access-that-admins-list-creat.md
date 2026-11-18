---
id: 0031
name: "Roles are records in `access` that admins list, create and delete, and grants must name one"
status: in-progress
spec: docs/platform/specs/0001-admins-assign-roles-to-users-within-the-app.md
blocked_by: [docs/platform/tickets/0029-admins-grant-and-revoke-roles-actions-at-runtime-taking-effect-within-seconds-without-a-deploy.md]
blocks: [docs/platform/tickets/0032-a-principal-s-roles-are-user-plus-its-subject-s-as.md]
---

# Roles are records in `access` that admins list, create and delete, and grants must name one

Roles become records owned by the platform module `access` (`crates/platform/access`). They share grants' revision, snapshot and topic, and they get operations, MCP tools, and a **Roles** tab on ticket 0029's admin screen. Principals still take their roles from the token here: that changes in the next Ticket. This Ticket makes role names real, so a grant to a misspelt role is refused.

Scope:
- **Storage.** `access.role` holds a name (the id, which can't be changed) and a description. `admin` and `user` are built in.
- **Operations.** `list_roles`, `create_role` and `delete_role`, each with an agent-facing summary (ADR 0034). `delete_role` takes `If-Match`, as revoking a grant does.
- **Events and revision.** `RoleCreated` and `RoleDeleted` go on `access`'s topic. Each change bumps the revision in its own Unit of Work.
- **Grants.** `grant_access` refuses a role that doesn't exist with `400`, naming the `role` field.
- **Defaults.** `migrate` creates any role a Module's default grants name that doesn't exist yet. It does so once, like the defaults themselves.
- **Screen.** A **Roles** tab beside **Grants**: a list, a create form, and delete with confirmation, showing refusals. The grant dialog's role field becomes a picker fed by `list_roles`.

## Acceptance criteria
- [ ] On a fresh database `admin` and `user` exist, and deleting either is refused.
- [ ] Creating a role whose name exists is refused `409`, and the revision doesn't move.
- [ ] A grant naming a role that doesn't exist is refused `400`, naming the `role` field.
- [ ] Deleting a role that a grant still names is refused `409`. After the grant is revoked, the delete succeeds.
- [ ] Creating and deleting a role each bump the revision in the change's Unit of Work and publish `RoleCreated` / `RoleDeleted`.
- [ ] `migrate` creates the roles that default grants name (for example `notes`' `support`). If an admin deletes such a role (after revoking its grants), the next `migrate` doesn't create it again.
- [ ] `list_roles`, `create_role` and `delete_role` appear in the spec and as tools on `/mcp`. After `make init` (`make check-init`), `access`'s operations are still the only ones.
- [ ] Against MSW in `src/test/`, the screen's Roles tab lists, creates and deletes (with confirmation) roles and shows a `409` refusal. The grant dialog offers only existing roles.
- [ ] Before this Ticket's PR is merged, `docs/platform/glossary.md` gains **Role**.
- [ ] `make generate` is committed, and `make check-changed` passes.
