---
id: 0001
name: Align agent docs with the code and make `make check` mirror CI
status: done
priority: P1
size: S
blocked_by: []
blocks: []
source: architecture review 2026-10-31, finding 3
---

# Align agent docs with the code and make `make check` mirror CI

## Description
`CLAUDE.md` and `spec.md` have drifted from the code. An agent following them
builds a module that doesn't match its siblings, or has to read every module to
work out which is right, which defeats the "minimal context" goal. `make check`
claims to be the CI mirror but skips steps CI runs, so "done locally" can still
fail CI.

## Evidence (as of 2026-10-31)
- **Repository shape is out of date.**
  - `CLAUDE.md` describes `get → Option<Aggregate>` / `save(&Aggregate)`; the code has
    `get → Option<Versioned<X>>` and `save(&X, Expected) -> Version`
    (`crates/modules/users/core/src/application/repository.rs`).
  - `CLAUDE.md` and `spec.md` §4/§6 say "`save` is an upsert"; it is an INSERT or a
    version-guarded UPDATE.
- **Optimistic concurrency is undocumented.** `CLAUDE.md` never mentions `Versioned`,
  `Expected`, `WithETag` or `If-Match`, yet every write module uses them. `spec.md` D6
  still calls a version column optional.
- **`Refusal` variants are wrong.** The docs list `Invalid | NotFound | Conflict | NotSignedIn`;
  the drivers also have `PreconditionRequired` and `Stale`.
- **`spec.md` has stale sections.**
  - D5 is titled "Handlers take the pool via `Data<PgPool>`", which contradicts §7 and the code.
  - It mentions a `config` crate and "Repository conventions" in `app-db`; neither exists.
  - `app-persistence` is missing from the §5 table.
  - It says `make dev` depends on `migrate`; it depends on `seed`.
- **Leftover "frontend not scaffolded yet" branches** in `Makefile`, `ci.yml`,
  `Dockerfile` and `.gitignore`.
- **`make check` falls short of CI.** It runs neither `pnpm biome ci` nor `pnpm run build`;
  CI runs both.
- **The pre-commit hook skips the seed bin.** `lefthook.yml` clippy omits
  `--features server/seed`, so the `seed` bin is never linted before a commit.

## Proposed approach
1. Correct every drift item above against the code; the code is the truth.
2. Add a short "Optimistic concurrency" rule to `CLAUDE.md` and point to the notes
   module as the reference.
3. Add `biome ci` and the FE build to `make check`, and fix the lefthook clippy features.
4. Remove the "not scaffolded" branches; the frontend exists.
5. Shrink `CLAUDE.md` (always loaded, ~3k words) to rules plus pointers to reference
   files, and move the rationale to `spec.md`. Keep each fact in one file only.

## Acceptance criteria
- [x] Every claim in `CLAUDE.md` about repository/service/driver shape matches the code.
- [x] `make check` runs the same steps as CI (backend and frontend).
- [x] No "not scaffolded" conditionals remain.
- [x] `CLAUDE.md` is meaningfully shorter, with no fact stated in both it and `spec.md`.

## Open questions
- What's the target size for `CLAUDE.md`? (Suggest ≤ 1.5k words.) — Resolved: ≤ 1.5k (landed at ~1.47k).
- Should a CI check catch future doc drift (e.g. grep for known-dead identifiers), or is review enough? — Open: not added; review for now.

## Notes
Re-run this after each later ticket lands. Every structural change below also changes the docs.
