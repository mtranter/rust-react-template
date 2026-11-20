---
id: 0059
name: "Users start, list, open and delete their own Conversations"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0058-an-assistant-platform-module-reports-whether-the-a.md]
blocks: [docs/platform/tickets/0060-sending-a-message-runs-a-streamed-turn-that-reads.md]
---

# Users start, list, open and delete their own Conversations

The Conversation aggregate root (ADR 0030) in `assistant`, with its Messages and tool-call records stored but no turns yet: start, list (caller's own, newest first), get, delete. Owner-scoped like notes. The SPA's Assistant page lists the user's conversations with a "new conversation" button and delete, and opens one (showing its stored messages, empty for now).

## Acceptance criteria
- [ ] Migrations for conversations and their messages; a Message is user text, assistant text, or assistant tool calls with results; each tool call records tool name, arguments, status (`ran`, `awaiting_approval`, `approved`, `rejected`) and result or error body.
- [ ] A Conversation has a title (empty/placeholder until its first message), a version with `If-Match` on changes, a topic and events carrying its state (ADR 0012, 0016), and seeds, following the reference module's layers.
- [ ] Operations to start, list, get and delete, each with a snake_case `operation_id` and an agent-readable summary, gated by the module's chat action, and listed in `REST_ONLY`.
- [ ] Endpoint tests: a list holds only the caller's conversations; another user's is `404` on get and delete; admins get no read of others' by default; stale `If-Match` is `412`.
- [ ] SPA: the Assistant page lists conversations newest first, creates one, deletes one and opens one; feature tests cover these.
- [ ] `make generate` run and `openapi.json` committed; `make check-init` passes.
- [ ] `docs/platform/glossary.md` gains **Conversation**; `crates/AGENTS.md` updated as needed.
- [ ] `make check-changed` passes.
