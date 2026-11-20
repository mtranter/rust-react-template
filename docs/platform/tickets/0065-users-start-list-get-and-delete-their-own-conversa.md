---
id: 0065
name: "Users start, list, get and delete their own Conversations"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0063-the-assistant-platform-module-exists-and-reports-w.md]
blocks: []
---

# Users start, list, get and delete their own Conversations

The Conversation aggregate root (ADR 0030) with its Messages, stored per user and scoped to its owner (the token's `sub`). A Message is the user's text, the assistant's text, or the assistant's tool calls, each with tool name, arguments, status (`ran`, `awaiting_approval`, `approved`, `rejected`) and result or error body. A Conversation's title is, at first, the start of its first message. No turns yet: this ticket is the storage and the CRUD operations.

## Acceptance criteria
- [ ] Operations to start a conversation, list the caller's (newest first), get one with its messages, and delete one, each with a snake_case `operation_id` and summary, guarded by `chat`, and listed in `REST_ONLY`.
- [ ] Conversations carry a `version`; delete honours `If-Match` (`412` when stale, `428` when missing, as the reference module does).
- [ ] The model can represent every Message kind and tool-call status in the spec, with migrations and the module's topic and events carrying the Conversation's state (ADR 0012, 0016).
- [ ] Seeds add a conversation for a seeded user.
- [ ] Tests through the endpoints (ADR 0004): owner scoping — another user's conversation is `404` on get and delete and absent from their list; admins get no read of others' conversations.
- [ ] `docs/platform/glossary.md` gains **Conversation**.
- [ ] `make generate` run and `openapi.json` committed; `make check-changed` passes.
