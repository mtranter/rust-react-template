# Issue tracker: Local Markdown

Tickets for this repo live as markdown files, one per ticket, committed with the code, in two series that mirror the ADRs (`domain.md`): work on how the platform works in `docs/platform/tickets/`, work on the business domain in `docs/tickets/`. Each series is numbered on its own.

## Conventions

- One file per ticket: `NNNN-<slug>.md` in its series' folder, numbered on from the highest existing number in that folder (four digits, never reused).
- Each file opens with YAML front matter:

  ```yaml
  ---
  id: 0010
  name: One line stating what the ticket delivers
  status: needs-triage
  priority: P1        # P1 or P2
  size: M             # S, M or L
  blocked_by: []      # ticket ids, e.g. [0004]
  blocks: []
  source: where it came from (a review finding, a conversation)
  ---
  ```

- Then `# <name>` and the body: `## Description`, usually `## Evidence (as of <date>)`, `## Proposed approach`, `## Acceptance criteria`, and `## Open questions` or `## Notes` as needed. When it is finished, `## Resolution` records what was done and any decision taken (record one worth keeping as an ADR, `docs/agents/domain.md`).
- Triage and progress state is the `status:` field (see `triage-labels.md` for the role strings), plus `in-progress` while someone works on it and `done` when it lands. Closed tickets stay as history; don't rewrite them.
- Comments and conversation history append to the bottom of the file under a `## Comments` heading.

## When a skill says "publish to the issue tracker"

Create the next-numbered file in the series the work belongs to (`docs/platform/tickets/` or `docs/tickets/`) with the front matter above.

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the ticket number directly (`0007` is `docs/tickets/0007-*.md`, platform `0007` is `docs/platform/tickets/0007-*.md`).
