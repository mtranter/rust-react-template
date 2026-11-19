---
id: 0038
name: "Research agent identity, on-behalf-of tokens and an audit log, and draft a Spec for a human to decide"
status: in-progress
priority: P1
size: M
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 4.4): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Research agent identity, on-behalf-of tokens and an audit log, and draft a Spec for a human to decide

## Description

Agents call the app today with a user's token (ADR 0034), so the app can't tell an agent from the user it acts for. There is also no record of who changed what:

- the event envelope carries no actor (`crates/common/events/src/envelope.rs`);
- the outbox is compacted after each topic's retention, 7 days (ADR 0033);
- aggregates have no `created_by` or `updated_by`.

The human wants agents to act on behalf of users, via a standard, and a durable audit log, for humans and agents alike. They want a brief research doc first, then a Spec they will finish themselves.

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0006-agents-act-on-behalf-of-users-and-every-change-is-audited.md`, written as a Spec in `Needs Human Input`:

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

Also a **research doc**, `docs/platform/research/0001-agent-identity-and-audit.md`. It's brief, about two pages at most: options and recommendations, not a survey. It covers:

1. **The standards for acting on someone's behalf:**
   - OAuth 2.0 Token Exchange (RFC 8693): `act`, `may_act`, and how an actor chain looks in a token;
   - client credentials, for agents acting for no one;
   - how these fit MCP's authorization spec and the `/mcp` Protected Resource Metadata already served (`crates/common/web/src/mcp/discovery.rs`).
2. **Identity provider support and cost** for a solo founder running many single-tenant environments: at least Keycloak (self-hosted), Auth0, Okta, Entra ID (its own on-behalf-of flow), Amazon Cognito and Zitadel. Say what each supports and what it costs per environment.
3. **How a Principal carries an actor** (`crates/common/authz/src/principal.rs`, ADR 0040, ADR 0041).
   - Rules such as "agents may not delete", or "agents may approve only under an amount", need the policy language extended. Today a condition compares only a row's attributes (`R.attr`), with `==`, `!=` and `in`, so it can't condition on the principal alone and has no `<`.
   - Every condition must still compile to SQL (ADR 0040). Give the smallest extension that allows these rules.
4. **Audit log design options**, at least:
   - a table written in the same Unit of Work as the change;
   - an audit log built from events, with the actor added to the envelope;
   - Postgres triggers or an extension such as pgaudit.

   For each, say what a row records, how long it's kept, how it's read (the ops module, Spec 0005), and what it costs.

## Questions the human expects

The identity provider (it decides whether token exchange is available) and how long audit records are kept. Leave the rest to the research.

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0006-agents-act-on-behalf-of-users-and-every-change-is-audited.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
