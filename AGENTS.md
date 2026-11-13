# AGENTS.md

A clone-and-go **template**: a Rust modular-monolith backend (Poem + poem-openapi) whose OpenAPI spec drives a type-safe Vite + React + TypeScript frontend. Keep design decisions minimal; this is a template, not a product.

**Where the rules live.** This file is repo-wide. Before changing anything under `crates/`, read `crates/AGENTS.md` (backend architecture, modules, tests); before changing anything under `frontend/`, read `frontend/AGENTS.md` (features, forms, screens, UI). A change spanning both, such as a new module with a UI, follows both. Terms are defined in `GLOSSARY.md`; decisions, with the options weighed and their consequences, are recorded in `docs/adr/` (cited as ADR n). Each area's `AGENTS.md` names the reference code to copy rather than work from prose. If an `AGENTS.md` and the code disagree, the code wins: fix the `AGENTS.md` in the same change.

## Agent skills

- **Issue tracker.** Tickets are markdown files in `docs/tickets/`. See `docs/agents/issue-tracker.md`.
- **Triage labels.** The five default triage roles, recorded as each ticket's `status`. See `docs/agents/triage-labels.md`.
- **Domain docs.** Single-context: one `GLOSSARY.md` and `docs/adr/` at the root. See `docs/agents/domain.md`.

## Architecture and conventions

**Spec → client** (ADR 0006, 0023). The spec is generated from the poem-openapi handlers: never hand-write it. `frontend/src/api/` is generated from the checked-in `frontend/openapi.json`: never hand-edit it. After any backend API change, `make generate` and commit `openapi.json`; a change that breaks the frontend surfaces as a TypeScript error, and `make check-spec` fails while the snapshot is stale.

**One artifact** (ADR 0005). Production is one binary serving the API and the embedded SPA, with no separate static host. CI (`.github/workflows/ci.yml`) and `make check` run the same steps: a step added to one is added to the other. Lefthook runs rustfmt, clippy, biome and tsc before each commit.

### Commands

- `make setup` — once after cloning; installs the repo-local Rust tools (ADR 0027). `lefthook` is installed separately (`brew install lefthook && lefthook install`).
- `make dev` — migrate, seed, generate the client, then backend `:8080`, the `worker` and Vite `:5173` (which proxies `/api`).
- `make check` — every step CI runs, `check-backend` then `check-frontend`. **Run it before calling work done.** Run `pnpm install` in `frontend/` once first.
- `make check-changed` — only the halves of `make check` that the branch's changes since `main` can break (`scripts/check-changed.sh` says how it decides; a path it doesn't know runs both). It counts as `make check` for calling work done.
- `make generate` — dump spec → `frontend/openapi.json` → `frontend/src/api/`; `make spec` regenerates only the snapshot (DB-free).
- `make build`, and tests: `make test` (both), `make test-backend`, `make test-frontend`. Single tests and the rest are in each area's `AGENTS.md`; `make help` lists every target.

### Comments

Comments are the exception. Write one only when the code can't say it: a surprise (a workaround, a deliberate deviation, a non-obvious constraint), or the source or derivation of a value or algorithm. Don't restate the code, narrate steps or label sections; prefer a better name. Doc comments that only repeat the signature are the same noise.
