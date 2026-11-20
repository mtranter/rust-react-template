---
id: 0070
name: "The Assistant page chats, streaming text and read tool calls"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0066-sending-a-message-runs-a-streamed-turn-that-reads.md, docs/platform/tickets/0069-the-spa-s-assistant-page-lists-starts-and-deletes.md]
blocks: [docs/platform/tickets/0071-the-assistant-page-approves-and-rejects-pending-wr.md]
---

# The Assistant page chats, streaming text and read tool calls

A chat pane on the Assistant page: send a message and render the streamed turn. It consumes the stream through the generated client if `@hey-api` types `text/event-stream`, otherwise through a small hand-written reader in the feature using the generated types for event payloads.

## Acceptance criteria
- [ ] User and assistant messages show; assistant text is rendered as markdown safely (no raw HTML injection).
- [ ] Text streams in as it arrives; the input is disabled while a turn runs.
- [ ] Each tool call is a compact, expandable row: tool name, arguments, result or error.
- [ ] Errors and limit messages show inline; reopening a conversation shows its stored messages.
- [ ] Tests over a fake stream: a read answered; an error mid-stream.
- [ ] `frontend/src/api/` is not hand-edited; `make check-changed` passes.
