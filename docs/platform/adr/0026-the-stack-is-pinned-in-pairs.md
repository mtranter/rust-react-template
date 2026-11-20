# The stack is pinned, and poem and poem-openapi move as a pair

The template pins its stack so every clone builds the same: `poem` 3.1 with `poem-openapi` 5.1, `tokio` 1, PostgreSQL with `sqlx` 0.9, `thiserror` 2, `figment` for layered config, `tracing`; on the frontend Vite, React and TypeScript with pnpm, Biome, Vitest, TanStack Query 5, Router 1 and Table, and shadcn/ui.

Four pins carry a reason:

- **`poem` 3.1.x ↔ `poem-openapi` 5.1.x.** They are released in lockstep by date, not by a shared version number, so a mismatched pair fails to build. Bump them together.
- **The `opentelemetry` crates 0.33.x ↔ `tracing-opentelemetry` 0.34.x.** `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` and `opentelemetry-http` release together and often, and `tracing-opentelemetry` is built against one release of them, so a mismatch builds two `opentelemetry`s whose types don't meet. Bump them together ([ADR 0044](0044-traces-and-metrics-are-exported-over-otlp-and-a-trace-crosses-the-outbox-in-the-envelope.md)).
- **MSRV 1.97.1.** `poem` declares 1.85 and `sqlx` 0.9 1.94, but `rig-bedrock` 0.46 requires 1.97.1, so that is the workspace's `rust-version`; `rust-toolchain.toml`, CI, `mise.toml` and the Packer `rust_version` pin the same version.
- **hey-api packages exact.** `@hey-api/openapi-ts` and `@hey-api/client-fetch` are pre-1.0, so any release may change the generated code.

## Consequences

- A `cargo-deny` advisory with no fix reachable from the pinned stack is ignored with a note in `deny.toml`, to revisit on the next bump.
