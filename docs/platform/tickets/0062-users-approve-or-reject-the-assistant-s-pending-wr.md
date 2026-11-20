---
id: 0062
name: "Users approve or reject the assistant's pending writes"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0061-the-assistant-page-chats-streamed-replies-tool-cal.md]
blocks: []
---

# Users approve or reject the assistant's pending writes

The decide operation: approving a pending tool call runs it (as the user, with an `idempotency_key` derived from the tool call's id, ADR 0032), rejecting gives the model "the user declined this action", and the turn resumes and streams as send-message does. Sending a new message while calls are pending rejects them first. The SPA shows each pending write as a card with Approve and Reject.

## Acceptance criteria
- [ ] A decide operation (snake_case `operation_id`, summary, in `REST_ONLY`) approves or rejects one pending tool call and answers the turn's continuation as `text/event-stream`; deciding a call that isn't pending is `409` `conflict`; another user's conversation is `404`.
- [ ] Approved calls are recorded `approved` and run with an `idempotency_key` derived from the tool call's id; approving twice, or a retried request, changes nothing twice.
- [ ] Rejected calls are recorded `rejected` and the model receives "the user declined this action".
- [ ] Sending a new message while calls are pending records them as rejected before the new turn.
- [ ] Endpoint tests with the fake model: an approved write runs; a rejected write doesn't; a double approval changes nothing twice; deciding a non-pending call is `409`.
- [ ] Journey (ADR 0029): Alice asks the assistant to add an item to her note, approves, and the note has it.
- [ ] SPA: a pending write is a card showing the tool's summary and arguments with **Approve** and **Reject**, streaming the resumed turn; caches are invalidated after a write runs. Feature tests over a fake stream cover approve and reject.
- [ ] `make generate` run and `openapi.json` committed; `make check-init` passes; `make check-changed` passes.
