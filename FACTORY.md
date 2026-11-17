---
specStore: { kind: files, path: "./docs/platform/specs" }
ticketStore: { kind: files, path: "./docs/platform/tickets" }
statusMapping:
  specs:
    Draft: draft
    Ready for Tickets: ready-for-tickets
    Ticketed: ticketed
    Done: done
    Needs Human Input: needs-info
  tickets:
    Ready for Agent: ready-for-agent
    In Progress: in-progress
    In Review: in-review
    Done: done
    Needs Human Input: needs-info
codeHost: { kind: github, repo: mtranter/rust-react-template }
reviewers:
  - name: standards
    perspective: "Standards: does the change follow this repo's documented conventions (AGENTS.md, CLAUDE.md, GLOSSARY.md, docs/adr/, the Factory Config), and is it correct, readable, and adequately tested?"
  - name: spec
    perspective: "Spec conformance: does the change do everything the Ticket's acceptance criteria and its Spec ask for, and nothing they don't?"
maxReviewRounds: 5
check: make check-changed
mergePolicy: auto
maxConcurrentTickets: 2
compounder: { learningsThreshold: 5 }
---

# Factory Config

How the software factory works in this repo. The daemon reads the frontmatter above; agents and humans read this prose. Vocabulary and statuses are defined in the factory's glossary; `statusMapping` says how each canonical status is written here.

- **Specs** are markdown files in `./docs/platform/specs/` with a `status` frontmatter field. A Spec's Id is its path from the repo root, which is what its Tickets put in `spec`. Write new Specs with the `spec-creator` skill.
- **Tickets** are markdown files in `./docs/platform/tickets/` with the frontmatter `docs/agents/issue-tracker.md` describes, plus `spec` for a Ticket derived from a Spec. A Ticket's Id is its path from the repo root; a bare number in `blocked_by` means the Ticket file numbered so. The factory works only Tickets in the statuses mapped above; it ignores `needs-triage`, `ready-for-human`, and `wontfix`.
- **Code Host**: GitHub, `mtranter/rust-react-template`. Each Ticket gets its own branch, worktree, and PR.
- **Review**: `standards`, `spec` review every PR, for at most 5 rounds. Each round is one PR comment; together they are the Review History.
- **Merge gate**: once review passes, the daemon waits for CI, or runs `make check-changed` in the Ticket's worktree if no CI checks appear. It then merges the PR, one at a time.
- **Concurrency**: at most 2 Tickets run at once; the Delegator picks ones unlikely to conflict.
- **Learnings**: each Ticket's PR adds `.factory/learnings/<ticket>.md`. Once 5 are merged, or the factory is idle, the Compounder folds them into AGENTS.md, CLAUDE.md, the glossary, skills, or this prose, in a PR a human always merges.
- **Needs Human Input**: answer the recorded questions on the Spec or Ticket, then set it back to `ready-for-tickets` or `ready-for-agent`.
