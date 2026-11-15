---
id: 0012
name: Read modules keep their own copy of other modules' roots, fed from topics from the earliest
status: in-progress
priority: P2
size: L
blocked_by: [0011]
blocks: []
source: ticket 0011's follow-ups (PR #3, "Not in this PR")
---

# Read modules keep their own copy of other modules' roots, fed from topics from the earliest

## Description

Ticket 0011 made an `earliest` backfill cheap at any size, but nothing used it the way it exists for: a read module keeping its own copy of another module's roots. Each would have to hand-write the version guard and tombstones, couldn't tell when its copy was complete, and had no way to rebuild one in a new shape. `note-search` still joined the providers' published views, so the template had no reference.

## Evidence (as of 2026-11-15)

- `crates/AGENTS.md` "Not built yet": a `Projection` helper, a `make new-projection` scaffold, a readiness check on `messaging.backfill.completed_at`, and switching in a rebuilt `_v2` copy.
- ADR 0035's consequences: "A read module serving from one should check `completed_at` before relying on it; nothing does yet."
- `note-search` read `notes_api.note_v1` and `users_api.user_v1`.

## Proposed approach

ADR 0036.

## Acceptance criteria

- [x] `app_db::Project` / `Projecting`: the framework applies an event only when newer, keeping versions and tombstones in `messaging.projection_version`; tested in `tests/delivery/projection.rs`.
- [x] `app_db::caught_up` and `Generations`: a copy is served only once its backfill is copied and handled, and a rebuilt generation takes over once it catches up; `unavailable` (503) until then.
- [x] `note-search` keeps `note_search.note` and `note_search.author` from the notes and users topics; its tests publish the providers' events and cover renames, deletions and not being caught up.
- [x] The notes entity carries `createdAt`; seeds publish their creation events.
- [x] `make new-projection NAME=<module> FROM=<provider>` scaffolds a read module from `crates/projection-template` that compiles and passes its tests (tried against `notes`).
- [x] The search page says when search is still catching up.
- [x] ADR 0036; ADR 0010, ADR 0035, `crates/AGENTS.md` and the glossary point at it.
- [x] `make check` passes.
