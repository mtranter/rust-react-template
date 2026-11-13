# The stack is pinned, and poem and poem-openapi move as a pair

The template pins its stack so every clone builds the same: `poem` 3.1 with `poem-openapi` 5.1, `tokio` 1, PostgreSQL with `sqlx` 0.9, `thiserror` 2, `figment` for layered config, `tracing`; on the frontend Vite, React and TypeScript with pnpm, Biome, Vitest, TanStack Query 5, Router 1 and Table, and shadcn/ui.

Three pins carry a reason:

- **`poem` 3.1.x ↔ `poem-openapi` 5.1.x.** They are released in lockstep by date, not by a shared version number, so a mismatched pair fails to build. Bump them together.
- **MSRV 1.94.** `poem` declares 1.85, but `sqlx` 0.9 requires 1.94, so that is the workspace's `rust-version`; `rust-toolchain.toml` pins the latest stable.
- **hey-api packages exact.** `@hey-api/openapi-ts` and `@hey-api/client-fetch` are pre-1.0, so any release may change the generated code.

## Consequences

- A `cargo-deny` advisory with no fix reachable from the pinned stack is ignored with a note in `deny.toml`, to revisit on the next bump.
