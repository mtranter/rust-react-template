# AGENTS.md

A clone-and-go **template**: a Rust modular-monolith backend (Poem + poem-openapi) whose OpenAPI spec drives a type-safe Vite + React + TypeScript frontend. Keep design decisions minimal; this is a template, not a product.

**Where the rules live.** This file is repo-wide. Before changing anything under `crates/`, read `crates/AGENTS.md` (backend architecture, modules, tests); before changing anything under `frontend/`, read `frontend/AGENTS.md` (features, forms, screens, UI). A change spanning both, such as a new module with a UI, follows both. The platform's terms are defined in `docs/platform/glossary.md` and its decisions, with the options weighed and their consequences, in `docs/platform/adr/` (cited as ADR n); the business domain's, once there is one, in `GLOSSARY.md` and `docs/adr/`. Each area's `AGENTS.md` names the reference code to copy rather than work from prose. If an `AGENTS.md` and the code disagree, the code wins: fix the `AGENTS.md` in the same change.

## Agent skills

- **Issue tracker.** Tickets are markdown files: the platform's in `docs/platform/tickets/`, the business domain's in `docs/tickets/`. See `docs/agents/issue-tracker.md`.
- **Triage labels.** The five default triage roles, recorded as each ticket's `status`. See `docs/agents/triage-labels.md`.
- **Domain docs.** Two vocabularies: the platform's in `docs/platform/`, the business domain's at the root. See `docs/agents/domain.md`.
- **Task skills.** How-tos for specific tasks live in `.claude/skills/` (auth, events and projections, background work, telemetry, new modules, deployment, the identity provider). `crates/AGENTS.md` says which to load before which task: load it, even for a small change. A skill extends its area's `AGENTS.md`, never overrides it. If a skill and the code disagree, the code wins: fix the skill in the same change.

## Architecture and conventions

**Spec → client** (ADR 0006, 0023). The spec is generated from the poem-openapi handlers: never hand-write it. `frontend/src/api/` is generated from the checked-in `frontend/openapi.json`: never hand-edit it. After any backend API change, `make generate` and commit `openapi.json`; a change that breaks the frontend surfaces as a TypeScript error, and `make check-spec` fails while the snapshot is stale.

**Spec → client → tools** (ADR 0034). Every operation is also an MCP tool at `/mcp`, generated from the same spec: its `operation_id` is the tool's name and its summary the tool's description, so write both for an agent as much as for a person. There is no tool code to write; `crates/AGENTS.md` says how to keep an operation off MCP.

**One artifact** (ADR 0005). Production is one binary serving the API and the embedded SPA, with no separate static host. CI (`.github/workflows/ci.yml`) and `make check` run the same steps: a step added to one is added to the other.

### Commands

- `make init NAME=<name> TITLE="<display name>"` — once, first, in a fresh clone of the template, to start an app from it: the app's name and domain docs, the reference modules registered with no server, the UI down to a shell, in one commit that also removes `make init`. It runs `make setup` and ends in `make check`. In the template, `make check-init` (part of `make check` and CI, not of `make check-changed`) runs it on a scratch copy: run it after changing what `scripts/init.sh` edits.
- `make setup` — once after cloning; installs the repo-local Rust tools (ADR 0027), including the sccache that every checkout and worktree on the machine then compiles through (ADR 0037).
- `make dev` — migrate, seed, generate the client, then backend `:8080`, the `worker` and Vite `:5173` (which proxies `/api`).
- `make dev-observability` — `make dev` exporting its traces and metrics over OTLP to a local Grafana at `:3000` (compose's `observability` profile, which plain `docker compose up` and `make dev` leave off).
- `make check` — every step CI runs, `check-backend`, `check-frontend` then `e2e`. **Run it before calling work done.** Run `pnpm install` in `frontend/` once first, and `pnpm exec playwright install chromium` there where Playwright's Chromium is missing.
- `make e2e` — the end-to-end check (`scripts/e2e.sh`, `frontend/e2e/sign-in.ts`): a debug build of the server with the SPA embedded, on a freshly migrated and seeded database of its own, signed in to through a fake OIDC provider (`crates/common/fake-oidc`) in Playwright's Chromium; it opens `/access` as a bootstrap admin and signs out. Part of `make check` and CI, and of `make check-changed` when `frontend/`, `crates/server/` or `crates/common/web/` change.
- `make check-quick` — `cargo check` of every backend target: the inner loop after an edit, seconds rather than minutes. Not a substitute for the next.
- `make check-changed` — only the halves of `make check` that the branch's changes since `main` can break, and of the backend's tests only the changed crates' and their direct dependents' (a module's change runs its own tests and the journeys of what composes it; a change to `common` or the lockfile runs all). `scripts/check-changed.sh` says how it decides; a path it doesn't know runs everything. It counts as `make check` for calling work done.
- `make generate` — dump spec → `frontend/openapi.json` → `frontend/src/api/`; `make spec` regenerates only the snapshot (DB-free).
- `make build`, and tests: `make test` (both), `make test-backend`, `make test-frontend`. Single tests and the rest are in each area's `AGENTS.md`; `make help` lists every target.

### Comments

Comments are the exception. Write one only when the code can't say it: a surprise (a workaround, a deliberate deviation, a non-obvious constraint), or the source or derivation of a value or algorithm. Don't restate the code, narrate steps or label sections; prefer a better name. Doc comments that only repeat the signature are the same noise.



**Building**


Run only `make check-changed` during normal build cycles to minmimise build times.