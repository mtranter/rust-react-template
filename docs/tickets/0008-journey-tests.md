---
id: 0008
name: Add stateful journey tests beside the behaviour tests
status: todo
priority: P2
size: S
blocked_by: []
blocks: []
source: discussion 2026-11-01
---

# Add stateful journey tests beside the behaviour tests

## Description
Each behaviour test checks one rule from a seeded start, so nothing checks that a
module's verbs compose: that what one call returns is what the next one needs, and
that state carries through a sequence. A **journey** is one test that walks an
aggregate through its life, for example create → read → edit → read → delete →
read (not found), asserting at each step.

Behaviours stay the place where rules are tested. A journey checks the sequence
holds together; it doesn't repeat the rules, and a module has only a few journeys.

## Proposed approach
1. **Layout.** `tests/<module>/journeys/`, declared in `main.rs` beside `behaviours/`
   (`mod journeys { mod note_lifecycle; }`). The same target, driver and seed; a
   journey also starts from `X::seeded(pool)` and speaks only driver verbs.
2. **Rule carve-out.** Amend "one behaviour per test" (`CLAUDE.md` Testing, spec §13):
   a behaviour test checks one rule; a journey checks that a sequence of verbs
   composes, carries the version from each response into the next edit, and ends in
   a state the next read confirms. Name journeys for the path walked
   (`a_note_can_be_written_revised_and_removed`).
3. **Reference journey in `notes`.** Create → read → edit (with the returned version)
   → read → a stale edit is refused → list shows the edit → delete → read is not found.
4. **Scaffold.** Add the equivalent journey to `crates/module-template/tests/widgets/journeys/`
   so new modules start with one; `make check` runs it with the template's other tests.

## Acceptance criteria
- [ ] `notes` has a journey under `tests/notes/journeys/` that passes.
- [ ] The module template has one, and it passes.
- [ ] `CLAUDE.md` and spec §13 describe journeys and the carve-out from "one behaviour per test".
- [ ] `cargo test -p notes journeys::` runs only journeys.

## Open questions
- **Delete doesn't exist yet.** No module has a delete use case, so the example
  journey's last two steps need one. Either add `DELETE /api/notes/{id}` to `notes`
  (version-guarded with `If-Match` like edits; the template has no delete example
  at all, which is a gap of its own), or end the first journey at the stale edit and
  add the delete steps when 0007 settles deletion semantics ("deleting a user
  archives their notes"). Recommendation: add the delete here, as a hard delete;
  0007 can reuse it.
- Should `users` get a journey too (register → sign in → change display name →
  read)? It's the auth-guarded module, so it would show a journey crossing sessions.
