---
id: 0005
name: Enforce the module isolation rules the docs promise (backend and frontend)
status: done
priority: P2
size: S
blocked_by: [0004]
blocks: []
source: architecture review 2026-10-31, finding 6
---

# Enforce the module isolation rules the docs promise

## Description
"The Cargo graph enforces isolation" holds only until someone adds one line to a
`Cargo.toml`. Agents add dependencies freely to get something compiling. Several
documented rules have no check at all, on either the backend or the frontend.

## Evidence (as of 2026-10-31)
- **Core-to-core deps aren't banned.** Nothing stops `notes` adding `users = { workspace = true }`
  to `[dependencies]`. `deny.toml` only bans `sqlx`/`poem`, and the rule can't be
  expressed today because note-search legitimately depends on cores (fixed by 0004)
  and `deny.toml` uses `all-features = true`.
- **Write modules can declare `reads`.** "A write module never joins across schemas"
  is unchecked: `scripts/check-sql-schemas.sh` accepts a `reads` declaration from any module.
- **No frontend import rules.** Nothing checks "features never import each other" or
  "import `@/features/x`, never a file inside it"; `frontend/biome.json` has no
  import restrictions.

## Proposed approach
1. **A `cargo metadata` check** (e.g. `scripts/check-module-deps.sh` or an `xtask`), in
   `make check` and CI, that fails if:
   - a module core's normal dependencies include another module core (`server` is
     whitelisted, dev-dependencies are allowed);
   - an `-api` crate depends on anything but pure crates (already partly covered by deny);
   - a module with a `domain/` declares `[package.metadata.module] reads`.
2. **Frontend import rules.** Biome's `noRestrictedImports` (with path patterns, if the
   installed Biome supports it) or `eslint-plugin-boundaries`:
   - `src/features/a/**` may not import `@/features/b/**`;
   - outside a feature, `@/features/x/<file>` imports are refused (only the `index.ts`).
3. Consider merging `check-sql` and the new checks into one `make check-boundaries` target.

## Acceptance criteria
- [x] Adding `users` to `notes`' `[dependencies]` fails `make check`.
- [x] Adding `reads = [...]` to a write module fails `make check`.
- [x] A cross-feature import in `frontend/src/features` fails lint.
- [x] The spec §4 "Isolation enforcement" list matches what's actually enforced.

## Open questions
- Shell + `jq` over `cargo metadata`, or a Rust `xtask` (more robust, but a new workspace member)?
- Is "module kind" (write/read) worth declaring explicitly in metadata rather than inferring it from a `domain/` folder?

## Resolution
- Shell + `jq` over `cargo metadata` (`scripts/check-module-deps.sh`, `make check-deps`),
  matching the other `scripts/check-*.sh`; no new workspace member.
- Module kind stays inferred from `src/domain/`: a read module has none by definition (D8).
- Frontend: `frontend/scripts/check-feature-imports.mjs` (`pnpm run lint:imports`, run by
  `make lint-frontend` and CI). Biome 1.9.4's `noRestrictedImports` takes exact names only, and
  the script also catches relative `../other-feature/x` imports a pattern rule would miss.
- `check-sql` and `check-deps` stay separate targets: one greps SQL, the other reads the
  Cargo graph.
