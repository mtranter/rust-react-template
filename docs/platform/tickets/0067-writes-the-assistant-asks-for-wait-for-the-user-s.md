---
id: 0067
name: "Writes the assistant asks for wait for the user's approval"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0066-sending-a-message-runs-a-streamed-turn-that-reads.md]
blocks: [docs/platform/tickets/0068-journey-the-assistant-acts-for-alice-and-can-t-see.md, docs/platform/tickets/0071-the-assistant-page-approves-and-rejects-pending-wr.md]
---

# Writes the assistant asks for wait for the user's approval

Offer the surface's write tools (any non-`GET` operation) too, without running them: the tool call is stored `awaiting_approval`, the stream sends an approval-required event naming the tool and its arguments, and the turn ends. A decide operation approves or rejects a Pending tool call and resumes the turn, streaming. Approving runs the call as the user with an `idempotency_key` derived from the tool call's id (ADR 0032); rejecting gives the model "the user declined this action".

## Acceptance criteria
- [ ] All operations on the surface are offered as tools; non-`GET` calls are stored `awaiting_approval` and emit an approval-required event; several writes in one step are all pending together.
- [ ] A decide operation (snake_case `operation_id`, in `REST_ONLY`) approves or rejects a pending call and streams the continued turn; deciding a call no longer pending is `409` `conflict`.
- [ ] Each approved write carries an `idempotency_key` derived from its tool call's id, so a double approval or retried request changes nothing twice.
- [ ] Sending a new message while calls are pending records them as rejected first.
- [ ] Endpoint tests with the scripted fake model: a write waits, runs when approved and doesn't when rejected; a double approval changes nothing twice; deciding a decided call is `409`.
- [ ] Glossary gains **Pending tool call**; crates/AGENTS.md describes the approval rule.
- [ ] `make generate` run and `openapi.json` committed; `make check-changed` passes.
