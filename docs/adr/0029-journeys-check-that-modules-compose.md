# Journeys check that Modules compose

Each Module's tests stand in for whatever is across its boundary: a Subscriber's driver publishes a hand-built provider Event, a consumer stubs the provider's Module API trait, a Read Module joins views over seeded rows. So none of them fails when a publisher's payload drifts from what its Subscriber expects, a provider's real trait implementation differs from the stub, a write doesn't reach the views a Read Module joins, or the Server's own wiring (`modules.rs`, the providers passed in, the composed Worker's Subscriptions, real JWT auth) is wrong.

A Journey is one test that walks a path across Modules on the composed Server, every registered Module, the real Worker, real auth, nothing stubbed, asserting at each step, each step's result feeding the next. It is the one exception to "one behaviour per test": a behaviour test checks one rule within a Module; a Journey checks that Modules compose along a path. Journeys don't re-test rules, so they are few, only for paths that cross a boundary, and named for the path walked (`a_note_is_found_by_its_authors_current_name`, `deleting_an_account_archives_only_that_users_notes`).

They live only in `server`'s `journeys` target, with a driver of their own that follows the Module drivers' discipline: it migrates, seeds and registers Subscriptions for `server::modules()`, builds the real app and Worker over a fixed Clock, exposes cross-module verbs, and carries each version into `If-Match` itself. No route, method, status or header appears outside it.

## Considered options

- **Journeys per Module, or in the template.** A Journey depends on which Modules the app has, so neither a Module nor the template has any.
- **End-to-end browser tests.** Slower, and the composition being checked is the backend's. Playwright is a documented future variant.

## Consequences

- A Module that reacts to, provides for, or is read by another adds or extends a Journey. A path within one Module is a behaviour test in that Module (`notes`' `lifecycle.rs`).
- `composed_app.rs` stays beside them as a wiring probe over the Spec (ADR 0019), not a Journey.
