# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root.
- **`docs/adr/`**: read the ADRs that touch the area you're about to work in. Each states one decision, why, what was rejected and what follows from it. A later ADR may supersede part of an earlier one; it says so in its first lines.

The rules an agent must follow live in `CLAUDE.md`/`AGENTS.md`, and the reference code (`notes`, `users`, `note-search`, `frontend/src/features/notes`) shows them applied. If the docs and the code disagree, the code wins: fix the doc in the same change.

## File structure

This is a single-context repo:

```
/
├── GLOSSARY.md
├── docs/
│   ├── adr/
│   │   ├── 0001-each-module-is-its-own-crate.md
│   │   └── …
│   ├── agents/
│   └── tickets/
└── crates/, frontend/
```

## Use the glossary's vocabulary

When your output names a domain concept (in a ticket title, a refactor proposal, a hypothesis, a test name), use the term as defined in `GLOSSARY.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Record decisions as ADRs

A new decision someone would later ask "why this way?" about gets an ADR: the next number, `docs/adr/NNNN-<decision-as-a-kebab-title>.md`, titled with the decision itself; the context and decision in prose; `## Considered options`; `## Consequences`. Keep it short, and leave how-to to the rules and the code. Changing a decision is a new ADR that names what it supersedes; the old one stays.

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR 0013 (delivery ordered per key), but worth reopening because…_
