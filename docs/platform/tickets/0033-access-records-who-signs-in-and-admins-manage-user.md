---
id: 0033
name: "`access` records who signs in, and admins manage users' roles on a Users tab"
status: in-review
spec: docs/platform/specs/0001-admins-assign-roles-to-users-within-the-app.md
blocked_by: [docs/platform/tickets/0032-a-principal-s-roles-are-user-plus-its-subject-s-as.md]
blocks: []
---

# `access` records who signs in, and admins manage users' roles on a Users tab

`access` records each subject that authenticates, so admins can pick users from a list rather than typing `sub`s. A `list_subjects` operation and a **Users** tab on the admin screen show subjects with their roles. From the tab an admin can assign and remove roles, and can add a `sub` that hasn't signed in yet.

Scope:
- **Recording subjects.** On authentication, `access` upserts the subject: its `sub`, plus name and email when the token carries them, and when it was last seen. "Last seen" is accurate to within an hour, so most requests write nothing.
- **Operation.** `list_subjects`, with an agent-facing summary, returns each subject with its assigned roles. Bootstrap admins (`APP_BOOTSTRAP_ADMINS`) are marked as such. A `sub` that has assignments but has never signed in is included too.
- **Screen.** A **Users** tab beside **Roles** and **Grants**:
  - subjects with their roles, searchable by name, email or `sub`;
  - assign a role with a picker fed by `list_roles`, and remove one, showing refusals (`400`, `409`, `412`);
  - add an unseen `sub` by typing it in and assigning it a role;
  - bootstrap admins shown as such, with no way to remove their `admin`.

## Acceptance criteria
- [ ] After a user's first authenticated request, `list_subjects` includes their `sub`, with name and email when the token carried them.
- [ ] A second request from the same subject within the hour writes nothing (shown by a test). A request after the hour updates last seen.
- [ ] A `sub` assigned a role before it ever signed in is listed with that role. Its first sign-in adds its details without losing the assignment.
- [ ] Subjects in `APP_BOOTSTRAP_ADMINS` are listed as bootstrap admins with `admin`.
- [ ] `list_subjects` is in the spec and on `/mcp`, is refused to a non-admin (`403`), and `make check-init` still passes.
- [ ] Against MSW in `src/test/`, the Users tab:
  - [ ] lists subjects and filters them by name, email and `sub`;
  - [ ] assigns a role from the picker and removes one;
  - [ ] shows a `409` refusal;
  - [ ] adds an unseen `sub`;
  - [ ] offers no removal of a bootstrap admin's `admin`.
- [ ] The admin screen, with all three tabs, stays hidden from a user without `admin`.
- [ ] Before this Ticket's PR is merged, `docs/platform/glossary.md` gains **Subject**.
- [ ] `make generate` is committed, and `make check-changed` passes.
