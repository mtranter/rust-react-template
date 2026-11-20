---
id: 0061
name: "The Assistant page chats: streamed replies, tool-call rows and inline errors"
status: done
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0060-sending-a-message-runs-a-streamed-turn-that-reads.md]
blocks: [docs/platform/tickets/0062-users-approve-or-reject-the-assistant-s-pending-wr.md]
---

# The Assistant page chats: streamed replies, tool-call rows and inline errors

The SPA's chat pane in `frontend/src/features/assistant`: send a message and render the turn's stream as it arrives. Use the generated client if `@hey-api` types the `text/event-stream` response; otherwise a small hand-written reader in the feature using the generated types for the event payloads (`frontend/src/api/` is never hand-edited). Pending writes are shown as such but approving them is the next ticket.

## Acceptance criteria
- [ ] The chat pane shows user and assistant messages; assistant text is rendered as markdown safely (no raw HTML injection).
- [ ] Each tool call is a compact, expandable row with tool name, arguments, and result or error.
- [ ] Text streams in as it arrives; the input is disabled while a turn runs; errors and limit messages are shown inline.
- [ ] Reopening a conversation shows its stored messages and tool calls.
- [ ] When a turn's results include a write that ran, TanStack Query caches are invalidated (all is acceptable) so Notes shows the change without a reload.
- [ ] Feature tests over a fake stream: a read answered, and an error mid-stream.
- [ ] `frontend/AGENTS.md` describes the feature and how it reads the stream; `make check-init` passes; `make check-changed` passes.
