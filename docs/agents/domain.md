# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`docs/platform/glossary.md`**: the platform's vocabulary (Module, Aggregate Root, Topic, Projection, Driver…).
- **`GLOSSARY.md`** at the repo root, if there is one: the business domain's vocabulary.
- **`docs/platform/adr/`**, and **`docs/adr/`** if there is one: read the ADRs that touch the area you're about to work in. Each states one decision, why, what was rejected and what follows from it. A later ADR may supersede part of an earlier one; it says so in its first lines.

The rules an agent must follow live in `CLAUDE.md`/`AGENTS.md`, and the reference code (`crates/reference/`, and the feature `frontend/AGENTS.md` names) shows them applied. If the docs and the code disagree, the code wins: fix the doc in the same change.

## File structure

Two vocabularies, each with its own glossary and ADR series. The platform's is how every app built from this template works; the business domain's is what this app is about, and is absent until it has one.

```
/
├── GLOSSARY.md              # the business domain's terms (absent until there is a domain)
├── docs/
│   ├── adr/                 # the business domain's decisions, numbered from 0001
│   ├── platform/
│   │   ├── glossary.md      # the platform's terms
│   │   ├── adr/
│   │   │   ├── 0001-each-module-is-its-own-crate.md
│   │   │   └── …
|   |   ├── specs/           # the platform's specs
│   │   └── tickets/         # the platform's tickets
│   ├── agents/
│   └── tickets/             # the business domain's tickets, numbered from 0001
└── crates/, frontend/
```

## Use the glossary's vocabulary

When your output names a domain concept (in a ticket title, a refactor proposal, a hypothesis, a test name), use the term as defined in the glossary that owns it: a mechanism in `docs/platform/glossary.md`, a business concept in `GLOSSARY.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Record decisions as ADRs

A new decision someone would later ask "why this way?" about gets an ADR in the series it belongs to: one about how the platform works in `docs/platform/adr/`, one about the business domain in `docs/adr/`. Take the next number in that series, `NNNN-<decision-as-a-kebab-title>.md`, titled with the decision itself; the context and decision in prose; `## Considered options`; `## Consequences`. Keep it short, and leave how-to to the rules and the code. Changing a decision is a new ADR that names what it supersedes; the old one stays.

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts platform ADR 0013 (delivery ordered per key), but worth reopening because…_
