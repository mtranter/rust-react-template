# Learnings: 0040, a Spec for a human on MCP tool hints and untrusted fields

- Spec 0008 follows the pattern of Specs 0006 and 0007: `status: needs-info` only, a body written assuming each recommendation with "(question N)" markers, and a last section `## Needs Human Input (factory)`. Six questions came to about 130 lines.
- poem-mcpserver 0.3.1 is still the newest release (checked 2026-11-19). Its `Tool` has no `annotations`. Master's unreleased 0.4.0 adds resources (`resources/list`, `resources/read`, a `Resources` trait) but no annotations, and expects poem 4, which isn't released. That clashes with ADR 0026's pinned poem 3.1 / poem-openapi 5.1 pair. The source is in `~/.cargo/registry/src/*/poem-mcpserver-0.3.1`.
- poem-openapi 5.1.16's `MetaSchema` has no extensions, so no `x-` key can be put on a field through the derive. The routes around this are a newtype with a `format`, or post-processing the spec JSON, as `app_web::with_error_shape` already does.
- The MCP hint defaults (schema.ts 2025-11-25) are `readOnlyHint` false, `destructiveHint` true, `idempotentHint` false and `openWorldHint` true. A server that sends no hints therefore looks maximally dangerous.
- `rmcp` 3.11.0 has `ToolAnnotations` and resources, but it targets tower and axum, not poem.
- `deny.toml` has `unknown-git = "deny"`, so a `[patch.crates-io]` must be a path (vendored crate), not a git fork.
- Each module's `OPERATIONS` table (`Performs`, `crates/common/web/src/caller.rs`) is already the per-operation metadata a module declares. It is a natural home for further per-operation flags.
