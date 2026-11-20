---
id: 0063
name: "The `assistant` platform module exists and reports whether the assistant is on"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: []
blocks: []
---

# The `assistant` platform module exists and reports whether the assistant is on

Add the platform module `assistant` (`crates/platform/assistant/{core,api}`, with the `new-module` skill), registered like `access` and kept by `make init`. It reads its configuration from the environment and serves the status operation, which tells a signed-in user whether the assistant is on (`APP_AGENT_MODEL` set) and whether they may chat. The gate action `chat` is declared on the module's resource and granted to `user` in the module's `default_grants.yaml` (ADR 0040). No model or rig yet: this is the module's skeleton and its on/off switch.

## Acceptance criteria
- [ ] `crates/platform/assistant/{core,api}` follow the reference module's layers and are registered in the server's composition, as `access` is.
- [ ] Configuration: `APP_AGENT_MODEL` (no default), `APP_AGENT_MAX_TOOL_CALLS` (10), `APP_AGENT_MAX_OUTPUT_TOKENS` (4096), `APP_AGENT_HISTORY_MESSAGES` (50) are parsed, with an invalid number refused at startup with a clear error; the server starts as usual with none of them set.
- [ ] A status operation (snake_case `operation_id`, summary written for an agent and a person) answers whether the assistant is on, guarded by the `chat` gate action; tests through the endpoint cover on, off, and a user whose `chat` grant is revoked.
- [ ] `chat` is granted to `user` by default and revocable at runtime through `access`.
- [ ] Every `assistant` operation is listed in `REST_ONLY` in `crates/server/src/tools.rs` with its reason (an external agent already has the tools directly; the turn operations stream).
- [ ] `scripts/init.sh` keeps the module; `make check-init` passes.
- [ ] `docs/platform/glossary.md` gains **Assistant**; `crates/AGENTS.md` describes the module.
- [ ] `make generate` run and `frontend/openapi.json` committed; `make check-changed` passes.
