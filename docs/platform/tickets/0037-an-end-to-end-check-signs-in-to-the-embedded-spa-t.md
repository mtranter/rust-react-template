---
id: 0037
name: An end-to-end check signs in to the embedded SPA through a fake OIDC provider and loads the access screen
status: ready-for-agent
spec: docs/platform/specs/0002-the-embedded-spa-signs-users-in-with-the-identity-provider-and-sends-their-token.md
blocked_by: [docs/platform/tickets/0035-a-visitor-signs-in-to-the-embedded-spa-with-the-id.md]
blocks: []
---

# An end-to-end check signs in to the embedded SPA through a fake OIDC provider and loads the access screen

Add the check that would have caught the shipped SPA getting `401` everywhere. Write a fake OIDC provider in Rust, in the test harness. It needs discovery, JWKS, an authorize endpoint that approves straight away as a named subject, a token endpoint and end-session. Add a Playwright test (Playwright is already a dev dependency, used by `make screens`) that runs a debug build of the binary with `embed-frontend` against a database seeded for the test. Its `APP_JWT_ISSUER` and `APP_OIDC_CLIENT_ID` point at the fake provider. Use the `access` screen, the one guarded screen every app keeps after `make init`. For example, make the subject an admin via `APP_BOOTSTRAP_ADMINS` so it can load data only its token can fetch.

## Acceptance criteria
- [ ] The fake OIDC provider is Rust code in the repo's test harness: no extra container or third-party image is needed. It issues RS256 tokens that the server's `JwtVerifier` accepts via JWKS.
- [ ] A Playwright test starts the fake provider and the debug `embed-frontend` binary on a freshly migrated and seeded test database. It signs in, opens `/access`, and asserts on data that only that subject's token could fetch. Then it signs out and asserts that the sign-in screen shows.
- [ ] A `make` target runs it and is part of `make check` (and listed in `make help`). `.github/workflows/ci.yml` runs the same step (ADR 0005 / AGENTS.md: a step in one is in the other).
- [ ] `scripts/check-changed.sh` runs it when `frontend/`, `crates/server/` or `crates/common/web/` change.
- [ ] The check still runs and passes in the app `make init` produces: `make check-init` passes.
- [ ] The root `AGENTS.md` and `frontend/AGENTS.md` mention the new check and how to run it.
- [ ] `make check-changed` passes.
