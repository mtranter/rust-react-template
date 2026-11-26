---
name: integrations
description: Make a module call another system's API (a CRM, a payment provider, a noticeboard) through a Connection an admin configures: declare the Connection, write the typed client over `connections_api::Clients`, call it from a job, write the stateful fake its tests and `make dev` use, and test it. Use before touching `Module::declaring`, `connections_api::{Declaration, Clients, Client, Request, DevConnection}`, `connections::clients`, a module's fake, or the Integrations screen.
---

# Integrations: calling another system (ADR 0050)

Extends `crates/AGENTS.md` (its "Connections" section holds the rules); if this file and the code disagree, the code wins: fix this file in the same change. The reference is `crates/reference/notes`' sharing to a noticeboard: copy it.

| Doing | Copy |
|---|---|
| Declaring the Connection | `crates/reference/notes/core/src/application/sharing.rs` (`noticeboard()`), its `di.rs` (`.declaring(noticeboard())`) |
| Calling it, from a job | `crates/reference/notes/core/src/application/sharing.rs` (`ShareNote`, `Run<ShareNote>`), the service's `share(..)` and `pin(..)` |
| Its stateful fake | `crates/reference/notes/fake` (`fake-noticeboard`: a lib for tests, a bin for `make dev` and `make e2e`) |
| Pointing `make dev` at the fake | `crates/reference/notes/core/src/seed.rs` (`dev_noticeboard()`), declared under `seed` in `di.rs`; the Makefile's `_dev-fake-noticeboard` |
| Testing it | `crates/reference/notes/core/tests/notes/`: the driver's `noticeboard_connected()` and `noticeboard_at(..)`, `behaviours/sharing.rs` |

## 1. Declare the Connection

In `application/`, a `Declaration` with a key unique across the app, a label an admin reads, the auth style, any non-secret fields and a health check: `Declaration::new("crm", "CRM").auth(Auth::Bearer).field("tenant", "Tenant").health_check(HealthCheck::get("/health"))`. Register it in `module(..)` with `.declaring(..)`. `migrate` records it; the admin's Integrations screen (`/integrations`, the platform's, kept by `make init`) then lists it as not configured. Changing it later redeclares it (dropping values it no longer names); removing it retires it. The auth style decides the secrets (`Auth::secrets`): don't declare a secret as a field.

Pick the health check a real system answers `2xx` cheaply and only with working credentials: it is what **Test connection** sends, and the only guard against the fake drifting from the real API.

## 2. Write the typed client

The module depends on `connections-api` (pure), takes `connections: fn(&Context) -> Arc<dyn Clients>` in its `module(..)`, and the server passes `connections::clients` (`crates/server/src/modules.rs`). Its service holds the `Arc<dyn Clients>`. Wrap the calls in methods taking and returning the module's own types: build a `Request` (`Request::put(format!("/pins/{id}")).json(&body)`, paths relative to the Connection's URL), `client(KEY).await?`, `send(request).await?`, then check `is_success()` and read `answer.json::<T>()?`. Every `CallError` converts into an `AppError` with `?`: unconfigured is `501 not_configured`, unreachable or unreadable `503 unavailable`. Any status is `Ok` from `send`: deciding what a `4xx` means is the module's.

Never put a secret, a URL or a response body in an error message, a span field or a log: `connections` already spans every call without them.

## 3. Call it from a job, or the request that needs the answer

Calling out in an outbox handler's unit of work holds its transaction open on another system. Prefer a deferred job (the `background-work` skill): the use case defers it in its `Tx` and answers at once; the job's `Run` calls out as `System`, and a failure is retried with backoff and dead-lettered in the end. Make it idempotent on the other side (a `PUT` by your id, or an idempotency key the system offers). Call in the request only when the caller needs the answer to proceed.

## 4. Write the stateful fake

A crate beside the module, `crates/<...>/<module>/fake`, package `fake-<system>`, added to the workspace members and dependencies and to `deny.toml`'s `poem` wrappers (it is never a dependency of shipped code: `crates/reference/notes/core` takes it as a dev-dependency only). Copy `fake-noticeboard`:

- A lib: the system's state in memory behind a `Mutex`, `endpoint()` serving only the operations the module uses, with the real API's paths, status codes and auth, refusing a wrong credential as the real one would; `serve()` on `127.0.0.1:0` on a thread with its own runtime, answering its URL; and accessors tests assert on (`pins()`, `puts()`).
- A bin: `--token <secret> [--port <port>]`, printing its URL as its first line once listening (what `make e2e` reads).
- Its own test of what it serves.

Keep it to what the module calls: it is code you maintain, and it drifts.

## 5. Point `make dev` at it

Under the module's `seed` feature, declare a `DevConnection` beside the `Declaration`: `.declaring(seed::dev_noticeboard())`, its URL the fake's fixed dev port and the secret the fake is started with. The `seed` bin gives it to the Connection only while it has no URL, so an admin's values are never overwritten, sealing its secrets with `CONNECTIONS_MASTER_KEY` (`make dev` sets a dev-only one). Add the fake to the Makefile's `DEV_PROCESSES` (`_dev-fake-noticeboard`). Real sandbox credentials are entered on the Integrations screen by a person, never seeded.

## 6. Test it

Tests are the module's own, outside-in (`crates/AGENTS.md` "Tests"): its driver composes the real `connections` (a dev-dependency: its migrations in the `TEMPLATE`, `connections::declare(..)` of the module's declaration there, `connections::clients_with(&ctx, keys)` handed to `provider(..)` and `handlers(..)`), and serves the fake in-process. A verb connects it as an admin would, through `connections::configure_for_dev_with(..)`. Cover: the job pins through the fake, a retry or a repeat changes the other system once, the Connection unconfigured or refused leaves the job failed to retry (`messaging.job`'s `last_error`), and another user's record is refused before anything is deferred.

`make e2e` connects one integration through the screen (`E2E_FAKE` in `scripts/e2e.sh`: the fake's bin, the Connection's key and its secret's name). An app's first integration points it at its own fake.
