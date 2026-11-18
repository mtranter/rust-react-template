---
status: done
---

# Admins assign roles to users within the app

## Problem

ADR 0040 takes a user's roles only from the token's `roles` claim, so who has which role is managed in the identity provider. Many providers don't issue that claim, or nest it under another name. On such a provider every signed-in user gets `403` from every guarded operation, and an admin can't change anyone's role from inside the app. Role names also aren't stored anywhere, so a grant to a misspelt role is accepted and silently does nothing.

No version of the app is deployed, so nothing depends on the token's roles.

## Solution overview

The platform module `access` (ticket 0029) grows to own roles and who has them, as well as grants:

- **Roles** are records that admins create and delete.
- **Assignments** map a subject (the token's `sub`) to roles.
- **Subjects** are the users `access` has seen sign in, so admins can pick from a list.

A Principal's roles are now its assignments plus `user`, and the token's `roles` claim is ignored. Roles, assignments and grants share one revision and one compiled snapshot, so a change to any of them takes effect on every instance within the same 5-second bound as grants, and no request reads the database to resolve roles. ADR 0040 is amended to match.

## Behaviour

### Roles

- A role has a name (its id, which can't be changed) and a description.
- `admin` and `user` are built in and can't be deleted.
- Creating a role whose name exists is refused with `409`.
- A grant or assignment naming a role that doesn't exist is refused with `400`, naming the field.
- Deleting a role that a grant or assignment still names is refused with `409`. Admins revoke those first, so nothing disappears silently.
- When `migrate` applies a module's default grants, it creates any role they name that doesn't exist yet. This happens once, like the defaults themselves: a role an admin later deletes isn't created again.

### Principal

- Every authenticated Principal has `user`, plus the roles assigned to their subject.
- `user` is implicit, so it can't be assigned or removed; trying is refused with `400`.
- The token's `roles` claim is no longer read. `Claims` loses `roles`, and tests and journeys give a user roles by assigning them through `access` instead of putting them in the token.
- `Principal::System` is unchanged.

### Subjects

- `access` records each subject that authenticates, with name and email when the token carries them and when it was last seen.
- "Last seen" is accurate to within an hour, so most requests write nothing.
- An admin can also assign roles to a `sub` that hasn't signed in yet, typed in, for example to set someone up before their first sign-in.

### Admins and lockout

- `APP_BOOTSTRAP_ADMINS` is a comma-separated list of subjects that always have `admin`, whatever is assigned. This is how a fresh app gets its first admin. These subjects are shown as such and can't be unassigned.
- Removing the last stored `admin` assignment is refused with `409` when `APP_BOOTSTRAP_ADMINS` is empty.
- As in ticket 0029, `admin`'s access to `access` comes from baseline rules, not grants.

### Operations

Each has an `operation_id` and a summary written for an agent (ADR 0034), so each is also an MCP tool:

- list, create and delete roles;
- list subjects, with their roles;
- assign a role to a subject, and remove one.

Changes that can race take `If-Match`, as grants do. Each change bumps the revision in its own Unit of Work and goes on `access`'s topic: `RoleCreated`, `RoleDeleted`, `RoleAssigned`, `RoleUnassigned`.

### Screen

- Ticket 0029's admin screen gains **Roles** and **Users** tabs beside **Grants**.
- Roles: a list, create, and delete with confirmation; refusals are shown.
- Users: subjects with their roles, searchable by name, email or `sub`; assign and remove roles, with a role picker fed by the role list; add an unseen `sub`.
- The grant dialog's role field becomes a picker of existing roles instead of free text.
- The admin screen stays hidden from anyone without `admin`.

### Seeds and `make init`

- Development seeds assign roles to their seeded users through `access`'s Module API, as `System`.
- After `make init`, `access` still has the only operations and tools, now including these.

### Docs

- ADR 0040 amended or superseded: roles come from `access`, and a `PrincipalResolver` from the token is no longer the default.
- `GLOSSARY.md` gains **Role**, **Assignment** and **Subject**.
- `.env.example` gains `APP_BOOTSTRAP_ADMINS`.

## Out of scope

- Reading roles from the token, or merging them with assignments.
- Groups, roles that expire, roles nested in other roles, and renaming a role.
- Bulk assignment, and import or export.
- An audit screen: the topic is the audit trail.
- Syncing users or roles to or from the identity provider.
- Multi-tenancy (ADR 0040 is single-tenant).

## Open risks

- **Order of work.** This builds on ticket 0029, which is in progress. Its role field, revision, snapshot and screen are what this extends, so the work must follow 0029 rather than run alongside it.
- **Name and email** depend on the provider putting them in the access token. Without them, the Users tab shows only `sub`.
