---
id: 0071
name: The Assistant page approves and rejects pending writes and refreshes affected data
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0067-writes-the-assistant-asks-for-wait-for-the-user-s.md, docs/platform/tickets/0070-the-assistant-page-chats-streaming-text-and-read-t.md]
blocks: []
---

# The Assistant page approves and rejects pending writes and refreshes affected data

Show a Pending tool call as a card with **Approve** and **Reject**, showing the tool's summary and its arguments, calling the decide operation and streaming the continued turn. When a turn's results include a write that ran, invalidate the affected TanStack Query caches (invalidating everything is acceptable) so Notes shows the change without a reload.

## Acceptance criteria
- [ ] An approval-required event (and a stored `awaiting_approval` call on reopen) renders a card with the tool's summary, arguments, Approve and Reject.
- [ ] Approve and Reject call the decide operation and stream the continued turn into the pane.
- [ ] After a write runs, query caches are invalidated and the Notes list reflects it without a reload.
- [ ] Tests over a fake stream: a write approved and a write rejected.
- [ ] `make generate` run if needed; `make check-changed` passes.
