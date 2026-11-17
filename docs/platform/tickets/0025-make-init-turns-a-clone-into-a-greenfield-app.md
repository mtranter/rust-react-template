---
id: 0025
name: `make init` turns a fresh clone of the template into a greenfield app with its own name, domain docs and no example features on its surface
status: done
priority: P1
size: L
blocked_by: []
blocks: []
source: conversation, 2026-11-16 (the template as the base of a software factory's business apps)
---

# `make init` turns a fresh clone of the template into a greenfield app with its own name, domain docs and no example features on its surface

## Description

A software factory clones this template as the starting point of each business app (a CRM, say). As cloned, it is the template: its name, its history, and the notes, users and note-search example modules served as the app's API, MCP tools and UI. Today every factory run would have to strip those by hand, differently each time.

`make init NAME=<crate-safe-name> TITLE="<display name>"` does it once, deterministically, right after cloning. It is not part of `make setup`. `setup` installs tools, and runs in every checkout and worktree (ADR 0037). `init` is destructive and runs once.

What `init` must **not** do is delete the example modules. They are still what agents copy: both `AGENTS.md` files are built around "copy `notes`", "copy `users`' `mail/`", "copy `note-search`". The platform journeys also exercise compaction and MCP through them. So `init` takes them off the product's surface while they stay compiled and tested, as `module-template` already is. They are retired later, once the app's own modules cover what they show.

## Evidence (as of 2026-11-16)

- **Already done.** The platform's docs sit apart from the app's:
  - `docs/platform/glossary.md`
  - `docs/platform/adr/` (0001–0039)
  - `docs/platform/tickets/`

  `docs/agents/domain.md` and `issue-tracker.md` describe the two series. So `init` leaves the docs as they are and only creates the app's own (a root `GLOSSARY.md`, `docs/adr/`, `docs/tickets/`).
- **The template's identity:**
  - `API_TITLE = "rust-vite-template API"` (`crates/server/src/lib.rs`)
  - `<title>rust-vite-template</title>` (`frontend/index.html`)
  - the opening of `AGENTS.md` ("A clone-and-go **template** … Keep design decisions minimal; this is a template, not a product.")
- **Example modules registered with the server:**
  - `crates/server/src/modules.rs` (`Endpoints` tuple, `modules()`)
  - `crates/server/Cargo.toml` (dependencies; the `seed` feature `["users/seed", "notes/seed"]`; dev-dependencies `users-api`, `notes-api`)
  - the root `Cargo.toml` workspace dependencies
  - the `deny.toml` wrapper lists
- **Example modules in the server's tests:**
  - `crates/server/tests/journeys/`: `finding_notes.rs` and `deleting_accounts.rs` (product-facing); `compacting_history.rs` and `agents.rs` (platform behaviour, driven through `notes`)
  - `driver.rs` and `composed_app.rs`
- **Example features in the frontend:**
  - `features/notes`, `features/note-search`
  - `pages/home-page.tsx`, `pages/search-page.tsx`, and their routes in `router.tsx`
  - `src/test/{notes-list,note-editing,note-search}.test.tsx` and `fixtures.ts`
  - `screens/scenarios.ts`
- **Docs that point at the examples:**
  - the reference table and rules in `crates/AGENTS.md`
  - `frontend/AGENTS.md`
  - `docs/agents/domain.md` (line 11)
  - `scripts/new-module.sh` (its closing hint names `notes::module(users::directory)`)

## Proposed approach

`scripts/init.sh`, run by `make init NAME=… TITLE=…`. It works on a clean tree only and ends in one commit.

1. **Guard and record.**
   - Refuse unless `NAME` (lowercase words joined by `-`, as `new-module.sh` checks) and `TITLE` are given, the tree is clean, and `TEMPLATE_VERSION` doesn't exist.
   - Write `TEMPLATE_VERSION`: the template commit cloned (`git rev-parse HEAD`) and its remote URL. That is the app's record of which platform it was cut from, for pulling later platform changes.
2. **The app's docs.**
   - Create a root `GLOSSARY.md` stub: `# <TITLE>`, one line saying it holds the business domain's terms (platform terms are in `docs/platform/glossary.md`), and empty `## ` sections to fill.
   - Create `docs/adr/` (a `.gitkeep`) and `docs/tickets/`, with ticket `0001-retire-the-reference-modules.md` (step 7).
3. **Identity.**
   - `API_TITLE` becomes `"<TITLE> API"`; `index.html`'s `<title>` and `frontend/package.json`'s `name` become `NAME`/`TITLE`.
   - The opening paragraph of `AGENTS.md` describes the app, not the template, and drops "keep design decisions minimal".
   - The database name and user (`app`) stay. They are local-only and generic, and changing them touches compose, the Makefile, CI, `.env.example`, `config.rs` and two scripts for no gain.
4. **Reference modules off the surface.**
   - Move `crates/modules/{notes,users,note-search}` to `crates/reference/`, and add `"crates/reference/*/*"` to the workspace members (they keep their package names, so `deny.toml`'s wrapper lists and `.sqlx` are unchanged).
   - Unregister them: `modules.rs` (empty `Endpoints` and `modules()`), `crates/server/Cargo.toml` (dependencies; the `seed` feature empty), and the root workspace dependencies only where the server needed them.
   - `check-modules.sh`, `check-module-deps.sh`, `check-sql-schemas.sh` and `check-changed.sh` treat `crates/reference/*/*` as they treat `crates/module-template/*`: compiled, linted and tested, but not registered.
   - Repoint `crates/AGENTS.md`'s reference table and rules, and `docs/agents/domain.md`, from `notes`/`users`/`note-search` to `crates/reference/…`.
5. **Server tests.**
   - Delete the product-facing journeys (`finding_notes`, `deleting_accounts`).
   - Keep `compacting_history` and `agents`, run over a test-only composition of the reference modules (a test-support `modules()` in the server's tests, not in `src/`), so the platform keeps its coverage.
   - `composed_app.rs`'s tool listing becomes the app's: empty.
6. **Frontend down to a shell.**
   - Delete the notes and note-search features, pages, routes, tests, fixtures and screen scenarios.
   - `router.tsx` keeps the root layout and a placeholder `/` page with the app's `TITLE`.
   - Keep `components/ui`, `components/data-table`, `lib/`, the theme and `render-app.tsx`.
   - `frontend/AGENTS.md`'s "copy" references point at the template's reference feature at the commit in `TEMPLATE_VERSION` (a GitHub URL pinned to that commit) until the app's first feature replaces them.
7. **Ticket 0001 in the app.** "Retire the reference modules". Done when the app's own modules cover each row of `crates/AGENTS.md`'s reference table (a root with children, a module API, a reaction, a projection, a read module, a port and adapter, idempotent retries) and its first feature covers `frontend/AGENTS.md`'s. Then repoint both tables to the app's code, move the two platform journeys onto the app's modules, and delete `crates/reference/`.
8. **Regenerate and prove.** `make spec`, `make generate`, then `make check`. `init` fails, leaving the commit unmade, if it doesn't pass.
9. **Leave no trace.** The commit deletes `scripts/init.sh` and the `init` target, and is titled `Initialise <TITLE> from the template at <short sha>`.
10. **Keep it working in the template.** A CI job (and the same step in `make check`, as AGENTS.md requires both to match) runs `init` on a scratch copy of the tree and then `make check` there. So a later platform change can't silently break it.

## Acceptance criteria

- [ ] `make init NAME=crm TITLE="Acme CRM"` on a fresh clone ends in one commit, and `make check` passes on it.
- [ ] Run again, or on a dirty tree, or without `NAME`/`TITLE`, it refuses and changes nothing.
- [ ] After it:
  - [ ] the spec has no operations and `/mcp` lists no tools;
  - [ ] the UI shows a placeholder home titled `Acme CRM`;
  - [ ] `make dev` starts and serves it;
  - [ ] `make new-module NAME=contacts` scaffolds a module that is registered and passes its tests.
- [ ] `crates/reference/` compiles, and its tests and the two platform journeys pass. No reference module is registered with the server.
- [ ] `TEMPLATE_VERSION`, the root `GLOSSARY.md` stub, `docs/adr/` and `docs/tickets/0001-*` exist. `docs/platform/` is unchanged.
- [ ] No file outside `crates/reference/`, `docs/platform/` and `TEMPLATE_VERSION` names `rust-vite-template`, `notes`, `users` or `note-search` (checked by a grep in the CI job).
- [ ] The template's CI runs `init` on a scratch copy and `make check` there, and `make check` runs the same step.
- [ ] `AGENTS.md` describes `make init` (before `make setup`) for the factory, and drops it from the app's copy.

## Open questions

- **A server with no modules.** `Endpoints` would be `()`. Check first that poem-openapi accepts it (spec dump, Swagger UI), along with `Idempotent<()>`, the MCP tool listing and `composed_app.rs`. If any refuses, fix the platform to accept an empty app rather than leave a placeholder module behind. This decides the shape of step 4, so do it first.
- **The `users` module.** Most business apps need people, and `users` is an account module with a mail port. Should `init` offer to keep it registered (`KEEP=users`)? Default: no, everything goes to `crates/reference/`, and an app scaffolds its own. ADR 0025 says not to build a user store unless asked.
- **Pinned upstream links.** They assume the template's remote is reachable by the factory's agents. If it's private, copy the reference feature to `frontend/reference/` (not built, not routed) instead.

## Notes

- **Health stays.** The health routes are in `app-web` and the server, which `init` doesn't touch.

## Resolution

- **An empty app works.** `Endpoints = ()` dumps a valid spec, and the composed-app tests and MCP pass with no tools. One gap: poem-openapi drops schemas no operation names, so the client lost `ErrorBody`/`ErrorCode`, which `frontend/src/lib/` is built on. `app_web::with_error_shape` adds them back to a spec that lacks them. `server::spec::<C>()` is what `dump-spec` writes and `/api/spec.json` serves.
- **Compositions.** `server::Composition` (an `Endpoints` type and `modules()`) is what each server function takes: `server::Registered` (in `modules.rs`) for the server, `worker`, `migrate`, `seed` and `dump-spec`, and `Tools<C>` for MCP.
- **The reference modules move in the template, not at `init`.** They live in `crates/reference/{users,notes,note-search}` and are registered in the template. `init` only unregisters them, so every doc names one stable path in both states. The journeys moved with them, into `crates/reference/journeys` (package `reference-journeys`). Its lib is the `Reference` composition, and its tests are all four journeys. The two product-facing journeys aren't deleted: they run over the reference modules, not the app's, and go with `crates/reference/` at retirement. That also keeps everything that names the reference modules inside `crates/reference/`, as the acceptance grep requires.
- **Workspace members** are `crates/*/*/core` and `crates/*/*/api`. Cargo reads a glob matching nothing as a path, so `crates/modules/*/*` failed while an app has no modules yet.
- **Scripts.** `check-modules` requires a reference module to be registered everywhere or nowhere. `check-module-deps`, `check-sql-schemas` and `check-changed` treat reference modules as modules. `new-module` handles the empty `Endpoints`, `modules()`, `seed` and `TOOLS` that `init` leaves. `scripts/migrate-unregistered.sh` (used by `make prepare` and `new-module`) migrates the module template's and reference modules' schemas, so `sqlx prepare` can check their queries once `make migrate` no longer does.
- **Fixed while proving it:** the Makefile now defaults `SQLX_OFFLINE=true`, as CI does; without a `.env`, a fresh clone's `make check` compiled `query!` against its empty database. `tsconfig.app.json` now types jest-dom's matchers for Vitest; they were only typed through tests that imported `vitest.setup.ts`.
- **`make check-init`** (`scripts/check-init.sh`; in `make check` and the CI `init` job, not in `make check-changed`) copies the working tree to `target/init-check/app` as a one-commit clone and checks that `init` refuses without NAME/TITLE, with a bad NAME, on a dirty tree, and a second time. It then runs `init` (which ends in the app's `make check`) and checks the commit, the new files, the unchanged `docs/platform/`, the empty spec, the title, that no `make init` is left, and the reference-name grep. Last, it runs `make new-module NAME=contacts` and its tests. It uses its own `init_check` database, never the dev one.
- **Open questions:** `KEEP=users` isn't offered (the default). The frontend reference is linked at the pinned commit on GitHub, or given as `git show <sha>:<path>` for any other remote.
- **One module isn't a tuple.** poem-openapi has no `OpenApi` for a 1-tuple, so `new-module` registers an app's first module bare (`type Endpoints = contacts::ContactsEndpoints;`) and makes it a pair with the second.
- `make dev` isn't automated. By hand, the initialised app migrated, seeded and served: `/health/ready` answered `ok`, and `/api/spec.json` served no paths and the error shape.
