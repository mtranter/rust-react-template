---
id: 0081
name: Admins manage Connections on an Integrations screen, and a reference integration with a stateful fake shows the pattern
status: ready-for-agent
priority: P1
size: L
blocked_by: [0079, 0080]
blocks: []
source: Conversation 2026-11-26, ADR 0050
---

# Admins manage Connections on an Integrations screen, and a reference integration with a stateful fake shows the pattern

## Description

ADR 0050, the screen and the reference. Read `frontend/AGENTS.md` first.

## Proposed approach

- **Integrations Area for admins:** a list with a status badge for each Connection. The form takes the URL, the fields and write-only secret inputs ("Set" or "Replace"), and has a **Test connection** button with a plain-language result.
- **A reference integration in `crates/reference`:** a module declares a Connection and calls it from a job. Its stateful fake is an in-memory server like `fake-oidc`, used by the module's tests, and `make dev` seeds the dev Connection to point at it.
- **A new skill, `.claude/skills/integrations`:** how to declare a Connection, write the typed client, write the fake and test it. Update `crates/AGENTS.md` to say when to load it.
- Check what `make init` should keep or strip (`make check-init`).

## Acceptance criteria

- [ ] e2e: an admin opens Integrations, enters a URL and a secret, tests the Connection and sees OK against the fake.
- [ ] The secret field never shows a stored value.
- [ ] `make check` passes.
