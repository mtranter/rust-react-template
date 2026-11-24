---
id: 0078
name: Each Area declares the action it needs, and users see only the Areas and Apps they may use
status: ready-for-agent
priority: P1
size: M
blocked_by: [0077]
blocks: []
source: ADR 0047 (accepted 2026-11-24), the third of three slices with 0074 and 0077
---

# Each Area declares the action it needs, and users see only the Areas and Apps they may use

## Description

ADR 0047, option B:
- Each Area in the shell's registry names the one action it needs.
- `access` tells the SPA which actions the caller has, through a new `get_my_permissions`.
- The shell shows the Areas whose action the caller has, and the Apps with at least one.

This replaces `useManagesAccess`. Hiding is a convenience: the server still refuses the operations themselves.

**Load the `endpoint-auth` skill first.** Read ADR 0047.

## Evidence (as of 2026-11-24)

- **Two kinds of operation exist today, and `crates/server/tests/composed_app.rs` checks both:**
  - **public**, listed in `PUBLIC` (`crates/server/src/public.rs`) with its reason;
  - **guarded**: every other operation must be named once in its Module's `OPERATIONS`, by an action its policies have (`every_guarded_operation_names_an_action_its_modules_policies_have`).
- **Admins reach `access` by built-in rules, not by grants:** e.g. `crates/platform/access/core/policies/subject.yaml` ("admins always list subjects, whatever is granted or revoked"). The grants table has no rows for them.
- **The SPA asks whether the caller manages access through `useManagesAccess`** (`frontend/src/features/access/use-manages-access.ts`), which succeeds when the Catalog answers. `shell/capabilities.ts` (after 0077) uses it to show Access.
- **Areas and the actions they need:**
  - notes: Notes `note` / `list`; Search `noteSummary` / `search` (`crates/reference/note-search/core/src/web/access.rs`);
  - access: Users `subject` / `list`; Roles `role` / `list`; Permissions `grant` / `list` (`crates/platform/access/core/src/web/access.rs`).

## What to build

1. **A third kind of operation: guarded by sign-in alone.**
   - Add `SIGNED_IN: &[(&str, &str)]` (operation id, reason) beside `PUBLIC` in `crates/server/src/public.rs`, renamed to `crates/server/src/exempt.rs` if that reads better, with a module comment saying what each list means.
   - `composed_app.rs` checks that each `SIGNED_IN` operation has a security requirement and is in no `OPERATIONS`. The guarded check skips `SIGNED_IN` as it skips `PUBLIC`. An operation listed in both fails.
2. **`get_my_permissions` in `access`** (`GET`, operation id `get_my_permissions`, summary written for an agent too).
   - It takes `app_web::Caller` and calls no `authorize`. It is listed in `SIGNED_IN` with the reason "it answers only what the caller may do, so revoking it would only hide the caller's own Apps".
   - It answers `{ permissions: [{ resource, action }] }`: every resource and action in the loaded policies for which the compiled rule set's plan for the caller is `Allowed` or `Where(..)`, not `Denied`. The plan is the same one the server enforces, so built-in rules count as much as grants. A Scoped grant counts: the caller can use the action on some rows. Sort by resource, then action.
   - Add it to `REST_ONLY` in `crates/server/src/tools.rs` (an agent over MCP gets its tools listed anyway) and to `platform_ops` in `scripts/check-init.sh`.
   - Then `make generate`.
3. **Tests in `access`:**
   - a `user` gets the default grants' actions;
   - a role granted one action at a Scope gets it;
   - `admin` gets `subject` / `list`, `role` / `list` and `grant` / `list` with no grant rows;
   - a revoked grant disappears;
   - no token answers `401`.
4. **Areas declare `needs`.** In `shell/capabilities.ts`, `Area` gains `needs: { resource: string; action: string }`, required, set as listed in Evidence. Then:
   - `useCapabilities` queries `get_my_permissions` once (`staleTime` 60s, like `useManagesAccess`). It keeps each Area whose `needs` the answer contains, and each Capability with at least one Area left. While pending, it shows nothing rather than everything.
   - The launcher, sidebar, switcher, breadcrumb and Go to read only that list. An Area link opened directly still routes; the server's refusal is what the page shows.
   - Delete `useManagesAccess`.
5. **SPA tests** (MSW for `get_my_permissions` in the shared handlers, defaulting to everything):
   - a user without `subject` / `list` sees Access with Roles and Permissions but no Users;
   - a user with no access actions sees no Access;
   - a user with only `noteSummary` / `search` sees Notes with Search alone.
   - Update `shell.test.tsx`, `router.test.tsx` and the screens.
6. **Areas name real actions.** `check-modules.sh` fails on an Area whose `needs` names a resource and action no Module's policies declare. Compare against `get_access_catalog`'s resources in the generated client types, or `server`'s `dump-spec` plus a small `server` binary flag listing them: pick the simpler, and say which in `check-modules.sh`'s comment. `init.sh`'s generated `capabilities.ts` and `new-module.sh`'s registry entry give each Area a `needs` (the scaffold's `<entity>` / `list`).
7. **Docs.**
   - `frontend/AGENTS.md`: Shell (Areas' `needs`, what is shown, hiding isn't protection).
   - `crates/AGENTS.md`: auth bullets (`SIGNED_IN`).
   - `docs/platform/glossary.md`: **Area** gains its required action.
   - The `endpoint-auth` skill.

## Acceptance criteria

- `get_my_permissions` answers exactly the actions the server would allow the caller on some row, built-in rules included, and nothing it would deny.
- `composed_app.rs` fails on an operation that is neither public, signed-in only, nor in an `OPERATIONS`, and on one listed twice.
- Users are shown exactly the Areas whose `needs` they hold, and the Apps with any such Area. The bootstrap admin sees Access with all three Areas in e2e.
- `make check-modules` fails on an Area naming an unknown action.
- `make check-changed` passes, including e2e. `make check-init` passes.

## Notes

- An Area needs exactly one action until one needs more ("any of", "all of").
- 0075 (disabling) and 0076 (embedded panels) are descoped. Don't build for them.
