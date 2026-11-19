---
id: 0043
name: "Draft a Spec, for a human to decide, for rate limits, quotas, request body limits and timeouts"
status: ready-for-agent
priority: P2
size: S
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 4.9): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Draft a Spec, for a human to decide, for rate limits, quotas, request body limits and timeouts

## Description

The app has no request body limit, no request timeout and no rate limit: nothing in `crates/server/src/app.rs` or `crates/common/web/src/` sets one. The JWKS refresh is also uncapped, so a token with an unknown key id makes the server fetch keys again every time (`Jwks::verify`, `crates/common/web/src/auth.rs`).

The human's plan:

- handle DDoS at the infrastructure level;
- possibly put API Gateway in front of everything, with a custom authorizer that also rate-limits, for quotas per tenant.

The review pushed back. A custom authorizer duplicates the app's authentication, adds latency and cost to every request, and can't see the actor behind an agent (Spec 0006). Its recommendation:

- WAF and CloudFront at the edge, shared across tenants;
- API Gateway for routing and coarse throttling, if at all;
- limits per principal and per actor inside the app, because in a single-tenant app the real risk is a runaway agent, not a DDoS.

The Spec must weigh both views fairly, with costs.

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0011-requests-are-limited-per-caller-and-in-size-and-time.md`, written as a Spec in `Needs Human Input`:

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

- **Where limits are enforced.** Options:
  - edge only;
  - API Gateway with a custom authorizer (the human's idea);
  - API Gateway throttling plus limits in the app;
  - the app alone.
- Default body size and timeout.
- What limits per principal and per actor apply by default, and where their counters live: in memory per instance, or in Postgres.
- Whether the JWKS refresh cap belongs here.

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0011-requests-are-limited-per-caller-and-in-size-and-time.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
