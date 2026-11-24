---
id: 0076
name: A Capability's panel can be embedded in a slot on another App's page, calling its owner's API
status: needs-triage
priority: P2
size: M
blocked_by: [0074]
blocks: []
source: conversation on 2026-11-24; ADR 0047's "Embed", the second way data appears in another App
---

# A Capability's panel can be embedded in a slot on another App's page, calling its owner's API

## Description

ADR 0047 lists three ways data appears in an App other than its owner's: Link, Embed and Copy. Link needs nothing new, and Copy is ADR 0038's enrichment or projection. Embed isn't built. A page names a **slot** (a contact's page: `contact.aside`). Another Capability contributes a **panel** to it (Resourcing: the person's skills). The panel renders there, calling its own Capability's API with the user's token, so its owner's grants decide what it shows. The backend has no coupling.

## Proposed approach

- **The shell wires panels to slots.** `frontend/AGENTS.md` forbids features importing each other (`lint:imports`), so a panel isn't imported by the page that hosts it. A slot is a typed name with its props (the record's id), declared by its host. A panel is registered against it, beside the Capability registry, with the Area-style `needs` that decide whether it shows.
- **A panel the user may not use renders nothing.** It doesn't show an empty box or an error. Its `needs` are read from `get_my_permissions` (ticket 0074).
- **Panels share one look**: a card with the owner's name and a link into the owner's App.
- **A reference slot and panel**, e.g. on the reference Capability's note page, so the factory has something to copy.

## Acceptance criteria

- A panel registered against a slot renders on the host page only for users granted its `needs`, calling only its own Capability's operations.
- The host's feature and the panel's feature don't import each other. `lint:imports` passes.
- `frontend/AGENTS.md` says when to Link, Embed or Copy, and how to add a slot and a panel.

## Open questions

- How does a slot pass its record across Capabilities when the ids differ (a contact versus the Person it refers to)? The host passes what it has, and the panel resolves it through its own API.
