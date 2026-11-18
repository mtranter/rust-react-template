---
id: 0027
name: Every operation has a Principal that passed its Module's gate and its action's policy, unless it is listed as public
status: in-progress
priority: P1
size: L
blocked_by: [0026]
blocks: [0028, 0029]
source: conversation, 2026-11-17 (an authz pattern at module, action and row level)
---

# Every operation has a Principal that passed its Module's gate and its action's policy, unless it is listed as public

## Description

ADR 0040's module and action levels, wired through the web and application layers, with `notes` as the reference.

- **Row level** (conditions on a loaded row, and lists) is ticket 0028. Until then a `Where` plan is treated as allowed here, and the ticket says so where it does.
- **Stored, editable grants** are ticket 0029. Until then the rule set is compiled once at startup from every Module's policy files and **default grants**, held in memory.

## Evidence (as of 2026-11-17)

- `JwtAuth(pub Claims)` is the only guard (`crates/common/web/src/auth.rs`), used only by `users`' `/users/me` operations (`crates/reference/users/core/src/web/handlers.rs`).
- `notes` takes no auth at all. `create_note` takes `authorId` from the request body, and anyone can edit, delete or list any note (`crates/reference/notes/core/src/web/handlers.rs`).
- Every Module's operations are served by one `OpenApiService` over the `Endpoints` tuple (`crates/server/src/modules.rs`). A Module registers through `Module` (`crates/common/module/src/lib.rs`), which holds no policies.
- `composed_app.rs` already checks every operation for an `operation_id` and summary, and lists the MCP tools. `REST_ONLY` in `crates/server/src/tools.rs` is the pattern for a reasoned allowlist.
- Ticket 0025 (`make init`) is moving `notes`, `users` and `note-search` to `crates/reference/`. This ticket's paths assume it has landed.

## Proposed approach

1. **Principal.**
   - The `JwtAuth` checker resolves `Claims` through a `PrincipalResolver` from request data (an `Arc<dyn PrincipalResolver>`, provided in `server::app::api` beside the `JwtVerifier`).
   - `Claims` gains an optional `roles: Vec<String>`, so `JwtVerifier::issue` gains roles for tests and journeys.
   - The default resolver maps `sub` to `id` and `roles` to `roles`.
2. **Registration.** `Module::with_policies(|| Policies)`:
   - The server and the worker build every Module's at startup, and compile one `RuleSet` from them and their default grants into an `Authorizer` in request data and in each service.
   - A policy that fails to load stops the server, the worker and `migrate`, naming the Module.
   - `make check-modules` fails on a Module with operations but no policies.
3. **Gate.** `app_web::Caller<G: Gate>` is a hand-written `ApiExtractor` delegating to `JwtAuth`, so the spec's security requirement is unchanged.
   - It plans `module:<G::MODULE>` / `use` and yields `Principal` on `Allowed`.
   - No token is `401`; `Denied` is `403`.
   - Each Module declares `pub struct Notes; impl Gate for Notes { … }` and `pub type Caller = app_web::Caller<Notes>;` in `web/`.
4. **Action.**
   - Services hold the `Authorizer` and take `&Principal` on every use case.
   - Each use case plans its action first and returns `AppError::Forbidden` on `Denied`, before anything is loaded or begun.
   - Reactions and seeds pass `Principal::System`.
5. **Operations name their action.**
   - Each Module's web layer has `OPERATIONS: &[Performs]`: an operation id with the resource and action it performs. That table is what ticket 0029's catalog shows an admin.
   - `composed_app.rs` fails on a guarded operation missing from its Module's table, or naming an action its policies don't have.
6. **`notes` as the reference.**
   - Policies: ADR 0040's `derived_roles.yaml` (the `author` scope), `note.yaml` (the archived DENY) and `default_grants.yaml` (`user` and `admin`).
   - `create_note` takes its author from the Principal and drops `authorId` from the request (a spec change: `make generate`, and the frontend's create form stops sending it).
7. **`users`.** `/users/me` takes `users`' `Caller`. `register_user` and `get_user` decide here whether they stay public, listed in `PUBLIC` with a reason, or not.
8. **Secure by default.** `PUBLIC: &[(&str, &str)]` (operation id, reason) in `crates/server/src/`. `composed_app.rs` fails on an operation with no security requirement that isn't listed, and on a listed one that has one.
9. **Scaffold.** `make new-module` (`crates/module-template/`) scaffolds a gate, a resource policy with no rules, default grants allowing `user` and `admin`, and an `OPERATIONS` table.
10. **Frontend and journeys.**
    - The dev sign-in and the journeys' tokens carry roles (`user`).
    - A journey shows a token without `user` refused `403` by `notes`.
    - Another shows a user who isn't the author still allowed to edit, until ticket 0028.
11. **Docs.**
    - `crates/AGENTS.md`: the rules (`Caller`, `&Principal` on use cases, an action and a default grant per operation, `OPERATIONS`, `PUBLIC`) and the reference table row ("A guarded operation and its policies" → `notes`' `policies/`, `web/`).
    - Glossary: **Principal**, **Policy**, **Grant**, **Scope**, **Gate**, **Plan**.
    - ADR 0025's "Consequences" points at ADR 0040.

## Acceptance criteria

- [ ] A `notes` operation without a token is `401`; with a token lacking `user`, `403`; with `user`, it is served.
- [ ] A note is created with the caller as its author, whatever the body says, and `authorId` is no longer in its request schema.
- [ ] A default grant naming an action not in `NoteAction` stops the server from starting, naming `notes`, the file and the action.
- [ ] An operation added without a security requirement fails `composed_app.rs` until it is listed in `PUBLIC`. One without an `OPERATIONS` entry fails it too.
- [ ] A seed and a reaction (archiving a deleted author's notes) still work as `System`.
- [ ] MCP: a tool call without `user` is an error result with code `forbidden`.
- [ ] `make new-module NAME=widgets` scaffolds a gate and policies that load, and its tests pass.
- [ ] `make generate` is committed, the frontend compiles and its tests pass, and `make check` passes.

## Open questions

- Whether `get_user` (fetch any user by id) should be public, gated, or removed from the reference.
