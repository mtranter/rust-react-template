---
id: 0040
name: "Draft a Spec, for a human to decide, for MCP tool hints and marking untrusted fields"
status: done
priority: P2
size: S
blocked_by: []
blocks: []
source: "conversation, 2026-11-19 (Review-2026-19-11.md, section 4.7): the human wants a Spec with questions to answer, not a Spec for the factory"
---

# Draft a Spec, for a human to decide, for MCP tool hints and marking untrusted fields

## Description

MCP tools can carry hints that let an agent, or the person approving it, judge a call before making it: `readOnlyHint`, `destructiveHint`, `idempotentHint` and `openWorldHint`. The app sends none. It also gives agents no way to tell text a user wrote (a note's body, which could hold a prompt-injection attack) from data the app itself controls.

The review proposes:

- deriving hints from each operation (its method, whether it takes an `Idempotency-Key`), with an override list beside `REST_ONLY` in `crates/server/src/tools.rs`, and a composed test listing every tool's hints;
- an `x-untrusted` schema extension on fields users write, and server instructions telling agents such fields are data, never instructions.

**The library can't do this yet.** poem-mcpserver 0.3.1's `Tool` (`protocol/tool.rs`) has only `name`, `description`, `input_schema` and `output_schema`, with no `annotations`. It also always answers `resources/list` with nothing and has no `resources/read`, which the ops module's runbooks (Spec 0005) would want.

This ticket's output is **a Spec for a human, not code**. The human will answer its questions and send it to the factory themselves.

## Deliverable

`docs/platform/specs/0008-tools-tell-agents-what-they-do-and-which-fields-are-untrusted.md`, written as a Spec in `Needs Human Input`:

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

- **How to get past the library.** Options include:
  - a newer poem-mcpserver, if one exists (check crates.io and its changelog);
  - a `[patch]` of the crate;
  - contributing upstream;
  - rewriting `tools/list` responses in `app_web::mcp`;
  - replacing the crate.

  Should runbooks move to MCP resources in the same change?
- Which operations count as destructive beyond `DELETE`, and who declares it.
- Whether a destructive tool needs a human's approval, and where that is enforced: hints only advise, and enforcement belongs to policy (Spec 0006).

Add others only where a decision changes the design.

## Acceptance criteria

- [ ] `docs/platform/specs/0008-tools-tell-agents-what-they-do-and-which-fields-are-untrusted.md` exists with `status: needs-info` and the shape above, ending in `## Needs Human Input (factory)`.
- [ ] Every question gives context, options with their costs, a recommendation and what depends on it.
- [ ] Facts about this repo cite their paths. Facts about outside services, standards and prices cite a source URL and the date checked.
- [ ] The Spec names no implementation tickets, and no tickets are created.
- [ ] Nothing else changes besides the Learnings file the factory asks for: no code, ADRs, `AGENTS.md`, `GLOSSARY.md` or other Specs.
- [ ] `make check-changed` passes.
