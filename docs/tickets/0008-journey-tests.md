---
id: 0008
name: Add cross-module journey tests on the composed app
status: todo
priority: P2
size: S
blocked_by: []
blocks: []
source: discussion 2026-11-01
---

# Add cross-module journey tests on the composed app

## Description
Each module's tests stand in for whatever is on the other side of a module
boundary. A subscriber's driver publishes a hand-built provider event
(`notes.user_deleted(ALICE)`), a consumer stubs the provider's `-api` trait, and a
read module joins views over seeded rows. So no test checks that the real
modules fit together. Nothing fails when:
- a publisher's payload drifts from what a subscriber expects (both sides stay green);
- a provider's real `-api` implementation differs from the stub its consumer was tested with;
- a write in one module doesn't show up through the views a read module joins;
- the server's own wiring is wrong: `modules.rs`, the providers passed in,
  subscriptions registered by the composed worker, or real JWT auth.

`composed_app.rs` only probes reads for a 500, so it catches missing wiring but
not wrong behaviour.

A **journey** is one test that walks a path across modules on the composed app
(every registered module, the real worker, real auth), asserting at each step.
Behaviour tests stay the place where rules are tested. A journey checks only that
modules compose, so there are few of them, and only for paths that cross a
boundary.

We don't add module-level journeys. Behaviour tests already go through every real
layer and chain verbs (create, then edit with the returned version), so a
journey inside one module would mostly re-check them.

`composed_reactions.rs` is already a journey. But it uses raw routes, status codes
and `If-Match` headers, which breaks the "driver alone knows HTTP" and "tests speak
the domain" rules. This ticket gives composed tests the same driver discipline
the module tests have.

## Proposed approach
1. **Layout.** One target, `crates/server/tests/journeys/`, with `main.rs`,
   `driver.rs`, and one file per journey area. Its database names come from the
   test path, so they can't collide with the module targets.
2. **Composed driver.** `App::seeded(pool)` runs migrations, seeds and
   `register_subscriptions` for `server::modules()`, then builds `server::build_app`
   and `server::worker` from a `Context` with a `FixedClock` (both take `&Context`,
   as module drivers do), so time moves only via `later(by)`. It exposes verbs that cross modules and act as a signed-in user
   (`app.as_user(ALICE).write_note(..)`, `.rename_myself(..)`, `.delete_account()`,
   `app.search_notes(q)`, `app.note(id)`, `app.run_pending_events()`). It carries
   each version from response to `If-Match` itself. It returns the module drivers'
   `XJson`/`Refusal` shapes over `app_web::testing`, so every refusal is checked
   for the error shape and request id. No route, method or status appears outside
   `driver.rs`.
3. **Move `composed_reactions.rs`** into the target as the first journey, rewritten
   in driver verbs. `composed_app.rs` stays as it is: it's a wiring probe over the
   spec, not a journey.
4. **Reference journeys.**
   - *A note is found by its author's current name:* Alice writes a note → search
     finds it under her name → she renames herself → search finds it under the new
     name and not the old. Crosses notes' write path, users' write path, and both
     published views joined by note-search.
   - *Deleting an account archives only that user's notes:* Alice deletes her
     account → her notes are unchanged until the worker runs → after it runs her
     note is archived, Bob's isn't, and nothing is dead-lettered. Crosses users'
     real `UserDeleted` payload into notes' real subscription.
5. **Rule carve-out.** Amend "one behaviour per test" (`CLAUDE.md` Testing, spec
   §13): a behaviour test checks one rule within a module. A journey, in
   `crates/server/tests/journeys/` only, checks that modules compose along a path:
   each step's result feeds the next, and every step is asserted. Name journeys for
   the path walked (`a_note_is_found_by_its_authors_current_name`). A journey that
   touches one module belongs in that module's behaviours. Add to "Adding a
   module": if the module reacts to, provides for, or is read by another, add or
   extend a journey.

## Acceptance criteria
- [ ] `crates/server/tests/journeys/` holds a driver and the two reference journeys,
      and they pass.
- [ ] `composed_reactions.rs` is gone, its journey rewritten in driver verbs.
- [ ] No journey file mentions a route, method, status code or header.
- [ ] `cargo test -p server journeys::` runs only journeys.
- [ ] `CLAUDE.md` and spec §13 describe journeys, where they live, and the
      carve-out from "one behaviour per test".

## Non-goals
- Journeys inside one module, or one in the module template: a journey depends on
  which modules the app has, so a template module can't ship a meaningful one.
- A welcome-mail journey: the composed worker wires `LogMailer`, so it would need a
  way to inject a Fake mailer into `server::worker`. Add it if that's wanted for
  other reasons.
- Browser end-to-end tests of the frontend.

## Open questions
- **Notes delete.** No longer a blocker: `users` has `DELETE /users/me`, which the
  deletion journey uses. Whether `notes` needs its own delete is a separate ticket.
