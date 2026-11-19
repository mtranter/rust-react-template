---
id: 0042
name: "Draft a Spec, for a human to decide, for hosting environments on AWS Lambda"
status: done
priority: P2
size: M
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 4.8): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Draft a Spec, for a human to decide, for hosting environments on AWS Lambda

## Description

Each customer gets a single-tenant environment, and the founder wants each one cheap, ideally costing close to nothing while idle. The app runs today as long-lived containers: the server, the worker, Postgres and optionally Valkey. Lambda could scale an idle environment to zero, but several parts hold state between requests:

- **The worker's poll loop.** Spec 0004 adds `worker --drain`, which a timer could run.
- **MCP sessions** are held in process (ADR 0034, `crates/common/web/src/mcp/sessions.rs`).
- **The `Bus` and live topics over SSE** (ADR 0039, ticket 0024).
- **A database pool per process,** so many Lambdas at once could open too many connections.
- **JWKS** is fetched at startup (`crates/common/web/src/auth.rs`).

The founder has said Lambda is open, not decided ("tbd").

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0010-environments-can-run-on-aws-lambda-and-scale-to-zero.md`, written as a Spec in `Needs Human Input`:

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

The Spec must include a **cost comparison per environment per month**, for idle, light and busy use. Compare:

- **Today's containers:** for example a small Fargate, Lightsail or EC2 instance, with RDS or a container Postgres.
- **Lambda:** with a serverless or auto-pausing Postgres, such as Aurora Serverless v2 with auto-pause, or Neon.

Include the fixed costs that dominate small environments: NAT gateways, RDS Proxy, WAF, and cold-resume latency. Cite current prices with the date checked. Keep the container host working: one codebase, two hosts.
## Questions the human expects

- Whether to do this now, later, or not at all, given the costs.
- Which database to use per environment.
- For each stateful part above, which option:
  - MCP stateless, or sessions kept in Postgres;
  - live screens via API Gateway WebSockets, response streaming, or polling only;
  - pooling.
- The tenant isolation model: one AWS account per tenant, or shared accounts.

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0010-environments-can-run-on-aws-lambda-and-scale-to-zero.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
