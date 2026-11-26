---
id: 0079
name: A connections module stores declared Connections, with secrets sealed behind a KeyProvider
status: in-review
priority: P1
size: L
blocked_by: []
blocks: [0080, 0081]
source: Conversation 2026-11-26, ADR 0050
---

# A connections module stores declared Connections, with secrets sealed behind a KeyProvider

## Description

ADR 0050, the storage slice. **Load the `new-module`, `endpoint-auth` and `events-and-projections` skills first.**

## Proposed approach

- Add a platform module, `crates/platform/connections` (`make new-module`), registered in `crates/server/src/modules.rs`.
- Add a declaration API in `connections-api`: key, label, auth style (none, bearer, basic, API key in a header or query parameter, OAuth 2 client credentials), named non-secret fields, and an optional health-check request. Modules declare at registration, and migrate upserts the declarations.
- Add a `KeyProvider` port that wraps and unwraps a data key, with `EnvKeyProvider` reading `CONNECTIONS_MASTER_KEY`. Seal secrets with AES-256-GCM (envelope encryption) and store the version of the key that sealed each one.
- Operations (all admin only): `list_connections`, `get_connection`, `update_connection` (URL and fields), `set_connection_secret` and `clear_connection_secret`. The last two are kept off MCP.
- Reads return `{ set, set_at, set_by }` for each secret, never its value.
- Events carry no secret values.

## Acceptance criteria

- [ ] Journey: an admin sets a URL and a secret, reads them back, and sees `set: true` but no value. A non-admin is refused.
- [ ] With `CONNECTIONS_MASTER_KEY` unset, setting a secret answers `501 not_configured`.
- [ ] The secret operations are absent from `/mcp`'s tool list.
- [ ] The stored ciphertext doesn't contain the plaintext, and unsealing it with a different key fails.
- [ ] `make generate`, `make check-changed` pass.
