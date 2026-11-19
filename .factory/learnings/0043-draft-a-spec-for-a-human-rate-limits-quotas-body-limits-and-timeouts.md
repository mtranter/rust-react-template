# Learnings: 0043, a Spec for a human on rate limits, body limits and timeouts

- Spec 0011 follows Specs 0006–0010: `status: needs-info` only, the body assumes each recommendation with "(question N)" markers, and it ends in `## Needs Human Input (factory)`. With one line per option and the options run together on one line for the shorter questions, seven questions fit in about 130 lines.
- **The app sets no limits at all** as of 2026-11-19: no `SizeLimit`, no timeout, no `Server::idle_timeout` (`crates/server/src/main.rs`), no pool `acquire_timeout`/`statement_timeout` (`app_db::connect`), and JWKS fetches use `reqwest::Client::new()`, which has no timeout.
- **Poem 3.1.12 has `SizeLimit` but no timeout middleware**, and `SizeLimit` answers `411` to a body with no `Content-Length`. Read the vendored source under `~/.cargo/registry/src/*/poem-3.1.12/src/middleware/` rather than guessing.
- **One unsafe request verifies its token twice:** once in idempotency (`verified_caller`, `crates/common/web/src/auth.rs`) and once in its `Caller`. So a token with an unknown `kid` makes two JWKS fetches. The same `verified_caller` is the seam for any middleware that needs the caller before the handler.
- MCP tool calls reach `api` (`crates/server/src/app.rs`) in-process, so a middleware inside `api` applies to REST and MCP alike; one on `build_app` would see only `POST /mcp`.
- Refusal codes an MCP client should know are listed in the instructions in `crates/server/src/tools.rs`; a new status (`429`, `413`) needs a line there.
