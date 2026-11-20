---
id: 0068
name: "Journey: the assistant acts for Alice and can't see Alice's note for Bob"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0067-writes-the-assistant-asks-for-wait-for-the-user-s.md]
blocks: []
---

# Journey: the assistant acts for Alice and can't see Alice's note for Bob

A journey in `crates/reference/journeys` (ADR 0029) over the composed app with the scripted fake model.

## Acceptance criteria
- [ ] Alice asks the assistant to add an item to her note; the write waits for approval; she approves; reading the note shows the item.
- [ ] Bob asks the assistant for Alice's note; the tool call is refused `404` and that error body is the tool result the model receives.
- [ ] No Bedrock call is made; `make e2e` is unchanged (assistant off).
- [ ] `make check-changed` passes.
