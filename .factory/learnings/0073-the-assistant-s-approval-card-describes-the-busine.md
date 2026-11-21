# Learnings: 0073, the approval card describes the business action

- **serde_json has no `preserve_order` here.** Any `serde_json::Value` schema (a tool's `input_schema`) has its
  `properties` sorted alphabetically. When order matters, read it from poem-openapi's `Registry`
  (`MetaSchema.properties` is a `Vec`). That is what `app_web::mcp::fields` does to give `AgentTool.fields`.
  Don't turn on `preserve_order` workspace-wide: it would change every `Value`-built output.
- poem-openapi writes a path template as `/orders/{id}` (from `#[oai(path = "/:id")]`), and that's what
  `AgentTool.path` holds. A tool's path arguments have the same names as the template's parameters.
- No operation in the app declares a `format` today: dates and ids are plain `String`s, and poem-openapi's `chrono`
  and `uuid` features are off. To test format handling, the assistant's test driver hand-implements
  `Type`/`ParseFromJSON`/`ToJSON` on `Formatted<F>` (with `MetaSchema::new_with_format`).
- The assistant treats a tool call that isn't on the surface as a read. It runs it and gets a `404`, and never stores
  it `awaiting_approval`. So a pending call with no proposal can only happen when the app drops a tool between a turn
  and a reopen. Endpoint tests can't easily produce one, so the SPA test covers that fallback.
- `make generate` leaves `frontend/src/api/` untouched in git (it's gitignored). Only `frontend/openapi.json` gets
  committed.
- The journey (`crates/reference/journeys/tests/journeys/assistant.rs`) runs the proposal against the real notes
  operations. Use it to check a change to target lookup against real path templates.
- **`make check-init` (CI's `init` job, not in `make check-changed`) fails on any whole word naming a reference
  module (the plural of "note" or "user", see `named=` in `scripts/check-init.sh`) outside `crates/reference/` and
  `docs/platform/`**, comments and `.factory/` included: init keeps platform code, which must not name the reference
  modules. Use a made-up example such as `/orders/{id}` in kept code and docs, and run that script's `git grep` over
  what you changed.
