---
id: 0058
name: "An `assistant` platform module reports whether the assistant is on, and the SPA shows its page only then"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0057-the-workspace-s-msrv-rises-to-rust-1-97-1.md]
blocks: [docs/platform/tickets/0059-users-start-list-open-and-delete-their-own-convers.md]
---

# An `assistant` platform module reports whether the assistant is on, and the SPA shows its page only then

Add the platform module `assistant` (`crates/platform/assistant/{core,api}`, laid out like `access`), registered in the composition and kept by `make init`, with its configuration, the model seam, and the status operation; and the SPA's `frontend/src/features/assistant` with an `/assistant` route and a header link shown only when the status operation says the assistant is on. No conversations or turns yet.

Scope:
- `rig-core` and `rig-bedrock` 0.46 join the workspace dependencies and are used by `assistant`: the provider sits behind one seam (a module trait or rig's `CompletionModel`), with a Bedrock implementation built from `APP_AGENT_MODEL` and the AWS SDK's default chain, and a scripted fake model for tests wired in through the module's driver.
- Configuration: `APP_AGENT_MODEL` (no default; absent = off), `APP_AGENT_MAX_TOOL_CALLS` (10), `APP_AGENT_MAX_OUTPUT_TOKENS` (4096), `APP_AGENT_HISTORY_MESSAGES` (50).
- A gate action (e.g. `chat`) on the module's resource, granted to `user` in `policies/default_grants.yaml` (ADR 0040).
- A status operation (snake_case `operation_id`, summary written for agents too) saying whether the assistant is on.
- All of `assistant`'s operations listed in `REST_ONLY` in `crates/server/src/tools.rs`, with the reason the Spec gives.
- Accepting the AWS SDK's `default-https-client` (rustls on aws-lc-rs): Cargo.toml comments and ADR 0005 corrected, `deny.toml` allows the new licences, the Dockerfile, CI and the Graviton dev box still build (cmake / C toolchain if `aws-lc-sys` needs it).

## Acceptance criteria
- [ ] `crates/platform/assistant/{core,api}` exist, follow `crates/AGENTS.md`'s module rules, and are registered in `crates/server/src/modules.rs`.
- [ ] With no `APP_AGENT_MODEL` the server starts as usual and the status operation says the assistant is off; with one set (and a fake/test model wired via the driver) it says on. Tested through the endpoints (ADR 0004).
- [ ] The model sits behind one seam in `assistant`; a scripted fake model exists for tests; no test or CI step calls Bedrock.
- [ ] The four `APP_AGENT_*` variables are read with the Spec's defaults, each overridable, and covered by config tests.
- [ ] The `chat` (or similarly named) gate action is granted to `user` by default.
- [ ] The assistant's operations are in `REST_ONLY` with a reason, and the composed-app MCP tests still pass.
- [ ] `rig-core`/`rig-bedrock` 0.46 are workspace dependencies; `deny.toml`, Cargo.toml comments and ADR 0005 reflect aws-lc-rs being accepted; the Docker image still builds.
- [ ] SPA: `frontend/src/features/assistant` with route `/assistant`; the header links to it only when the status operation says on, and the page is hidden/unreachable when off. Feature tests cover both, in `frontend/AGENTS.md`'s style.
- [ ] `make generate` run and `frontend/openapi.json` committed.
- [ ] `scripts/init.sh` keeps the module and the page; `make check-init` passes.
- [ ] `docs/platform/glossary.md` gains **Assistant**; `crates/AGENTS.md` and `frontend/AGENTS.md` describe the module and feature; the `run-and-deploy` skill lists the new environment variables, the Bedrock IAM permissions (`bedrock:InvokeModelWithResponseStream`, `bedrock:InvokeModel`) and the data-residency note.
- [ ] `make e2e` is unchanged (assistant off there) and `make check-changed` passes.
