---
id: 0041
name: "Draft a Spec, for a human to decide, for an eval harness that measures the agent docs within a token budget"
status: ready-for-agent
priority: P2
size: S
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 5): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Draft a Spec, for a human to decide, for an eval harness that measures the agent docs within a token budget

## Description

The human plans to restructure the agent docs: condense the ADRs and rules into a strong `CLAUDE.md`, with first-class reference code to copy. The review counters with a short `CLAUDE.md`, checks in place of rules wherever a rule can be checked, and task skills loaded on demand. Both agree that evals should decide.

The human is **worried about token spend**: an eval runs an agent through whole tasks, many times, for each variant. The Spec must make cost a first-class constraint, not an afterthought.

What exists to build on:

- the factory in `../trizzoop`: an Implementor, two reviewers and `make check-changed`;
- its Learnings files, `.factory/learnings/`, each a real slip that could become an eval case;
- `make new-module`;
- the reference modules in `crates/reference/`.

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0009-evals-measure-which-agent-docs-make-agents-succeed-within-a-token-budget.md`, written as a Spec in `Needs Human Input`:

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

The Spec must include a **cost model**: tokens and dollars for one run of one task, one variant's suite, and one comparison of variants. State the assumptions: the model, task size, the share of prompt cache hits, and reviewers on or off. Cite current prices with the date checked.
## Questions the human expects

- A monthly budget, and a cap per run.
- The model, or models, to run evals on.
- How many tasks, and which.
- How often to run: only when the agent docs change, nightly, or on demand.
- Whether the graders include the LLM reviewers (`standards`, `spec`) or only `make check` and hidden tests.
- How factory Learnings become eval cases.

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0009-evals-measure-which-agent-docs-make-agents-succeed-within-a-token-budget.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
