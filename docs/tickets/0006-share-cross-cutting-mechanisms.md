---
id: 0006
name: Share cross-cutting mechanisms instead of copying them per module
status: done
priority: P2
size: M
blocked_by: []
blocks: [0007]
source: architecture review 2026-10-31, finding 7
---

# Share cross-cutting mechanisms instead of copying them per module

## Description
Every module copies the same small mechanisms, and their bugs with them. The review
surface grows linearly with the number of modules, and some copies are already
wrong or block testing.

## Evidence (as of 2026-10-31)
- **Every unique violation gets the email message.** In `crates/modules/users/core/src/db/repository.rs`,
  `map_sqlx` maps *every* unique violation to "a user with that email already
  exists". The first new unique constraint will produce the wrong message.
- **`now()` is private and duplicated.** It lives in each service
  (`users/.../service.rs`, `notes/.../service.rs`) and the scaffold template.
- **Seed `at(minutes)` is duplicated** in all three seeds.
- **Sort enums are hand-written.** Each needs 2N variants plus a `SortParam` mapping
  (`users/core/src/web/models.rs`, `notes/...`, `note-search/...`).
- **Test-driver plumbing is duplicated.** Each driver re-implements `outcome()`, the
  View newtype, the page struct and the query-string fold (about 150–230 lines per driver).

## Proposed approach
1. **`app_db::map_db_error`. Done.** Maps a violated constraint, by name, to the
   `AppError` the repository gives it, falling back to `internal`. `users` maps
   `account_email_key`; `notes` has no constraints and uses `AppError::internal`.
2. **Shared `now()`. Done.** `app_domain::now()` replaces the per-service copies.
   A controllable clock is deferred: nothing reads the clock except `createdAt`, and
   injecting one costs wiring in every module. Spec §16 records the plan (one shared
   context passed to every `module(..)`, not an ambient global) for when a module
   gains time-based rules.
3. **Shared seed timestamps. Done.** `app_domain::seed_time(minutes)` replaces the
   `at()` copies (in `app-domain` because every seed already depends on it for
   `well_known_id`).
4. **~~A `sort_param!` macro in `app-web`.~~ Dropped.** It would save the two web-layer
   rows per sortable field, about 25 lines per list endpoint, but `macro_rules!` can't
   build the `TitleAsc`/`TitleDesc` idents without `paste` (unmaintained, denied) or a
   fork, and a derive needs a proc-macro crate. The hand-written `#[derive(Enum)]` shows
   readers how the spec gets its enum, and the listing tests catch a mismatched direction.
5. **Shared driver transport. Done,** as `app_web::testing` behind `app-web`'s `test`
   feature rather than in `app-test-support`: a poem-free crate can't own the
   request loop, and that was half the duplication. `TestApi` boots the endpoints as
   the server does, `Call::send` checks the error shape and hands the driver a
   `Refused { code, paths }` to map by code (not status, per §13), `Page<V>` is the
   list shape, and `json_newtype!` declares the response newtypes (a macro, as the
   orphan rule forbids `impl From<Note> for` a shared type). The test `View` is now
   `XJson`. Drivers went from 265/283/179 lines to 159/183/96.

## Acceptance criteria
- [x] No module defines its own `now()` or `at()`.
- [x] A second unique constraint yields its own message (covered by `crates/common/db/tests/map_db_error.rs`,
  on a scratch table: `users.account` has no second constraint to test with).
- [x] Each driver shrinks to roughly domain verbs plus view constructors.
