---
id: 0080
name: Modules call out through a Connection's client, and admins test a Connection from its health check
status: ready-for-agent
priority: P1
size: M
blocked_by: [0079]
blocks: [0081]
source: Conversation 2026-11-26, ADR 0050
---

# Modules call out through a Connection's client, and admins test a Connection from its health check

## Description

ADR 0050, the calling slice. **Load the `add-telemetry` and `background-work` skills first.**

## Proposed approach

- In `connections-api`, `client(key)` returns an HTTP client with the base URL, auth (with the OAuth client-credentials token cached until it expires), timeouts and tracing applied. Secrets are redacted in spans and logs.
- An unconfigured Connection returns `connection_not_configured`.
- `test_connection` runs the declared health check and records the status (`untested`, `ok`, or `failing` with the last error), which `list_connections` shows.

## Acceptance criteria

- [ ] Journey against a fake server: each auth style sends the right header or parameter.
- [ ] `test_connection` reports `ok`, and after a bad secret it reports `failing` with a readable message.
- [ ] No secret appears in captured spans or logs.
