---
id: 0039
name: "Draft a Spec, for a human to decide, for sending the app's events out to webhooks and EventBridge"
status: ready-for-agent
priority: P1
size: S
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 4.6): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Draft a Spec, for a human to decide, for sending the app's events out to webhooks and EventBridge

## Description

Today MCP works one way: agents call the app, and the app tells nobody outside it when something happens. The `Bus` (ADR 0039) only broadcasts between the app's own instances, for live screens. The human wants the app to take part in agentic workflows: an external agent, n8n, Zapier, Step Functions or a Claude agent hears that, say, an invoice was approved, and acts on it.

The review proposes **sinks**: an outbox subscription (ADR 0013) whose handler delivers each event outside the app. Delivery then gets the existing ordering per record, retries, dead letters, backfill and the ops surface (Spec 0005) for free. The two sinks proposed are:

- a **webhook**, signed per the Standard Webhooks spec;
- **Amazon EventBridge** (`PutEvents`).

Events are already CloudEvents 1.0 (`crates/common/events/src/envelope.rs`).

The human found the review's security point unclear, so the Spec must explain it plainly. Each event carries the **whole record** as it now is (ADR 0012, ADR 0033). Whoever registers a sink on a topic would receive every change to every record on it, including records they couldn't see in the app.

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0007-the-app-sends-its-events-to-external-workflows-through-sinks.md`, written as a Spec in `Needs Human Input`:

- **Frontmatter:** `status: needs-info`, and nothing else. It must **not** be `ready-for-tickets`: the factory mustn't ticket it. A human answers its questions with `/spec-creator`, edits it, and sets it `ready-for-tickets` themselves.
- **Shape:** `# <Title>`, `## Problem`, `## Solution overview`, `## Behaviour`, `## Out of scope`, `## Open risks`, as in `docs/platform/specs/0003-…` to `0005-…`, then a last section, `## Needs Human Input (factory)`, holding the questions as a numbered list.
- **Write the body assuming each question's recommended answer**, and mark each passage that depends on an answer as "(question N)". The human can then accept a recommendation without rewriting anything.
- **Each question is clear to someone who hasn't read the code.** In plain words, give:
  - why it matters;
  - two to four options, each with what it costs (money, effort, risk) and what it changes;
  - a recommendation, with the reason;
  - which parts of the Spec depend on the answer.

  Ask only what the repo, its ADRs and public documentation can't settle. At most 8 questions.
- **Brief:** the Spec is at most about 150 lines.

## Questions the human expects

- **Who may register a sink, and how is it kept from leaking records the registrant couldn't see?** At least these options:
  - only admins register, and accept that a topic is exposed whole;
  - a sink acts as a principal, and each event is checked against that principal's row access before it's sent;
  - thin events, carrying only ids, so the receiver fetches what it may see through the API.
- Which destinations are in v1.
- Whether a new sink receives history (backfill) or only new events.

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0007-the-app-sends-its-events-to-external-workflows-through-sinks.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
