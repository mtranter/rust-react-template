---
id: 0034
name: "The binary serves the SPA's sign-in settings at /auth/config and refuses to start in prod without a client id"
status: done
spec: docs/platform/specs/0002-the-embedded-spa-signs-users-in-with-the-identity-provider-and-sends-their-token.md
blocked_by: []
blocks: [docs/platform/tickets/0035-a-visitor-signs-in-to-the-embedded-spa-with-the-id.md]
---

# The binary serves the SPA's sign-in settings at /auth/config and refuses to start in prod without a client id

Add the server-side half of SPA sign-in: the new `APP_OIDC_*` settings in `AppConfig` (`crates/server/src/config.rs`), a `GET /auth/config` route mounted beside `/health` in `crates/server/src/app.rs` (outside `/api`, so not an operation, Spec entry or MCP tool), and the prod start-up refusal. Keep the JSON shape small and stable: the SPA ticket that follows reads it.

## Acceptance criteria
- [ ] `AppConfig` has `oidc_client_id`, `oidc_scopes` and `oidc_extra_params`, loaded from `APP_OIDC_CLIENT_ID`, `APP_OIDC_SCOPES` (extra scopes beyond `openid profile email`) and `APP_OIDC_EXTRA_PARAMS` (extra authorization request parameters, e.g. Auth0's `audience`), all optional; the format for scopes and extra params is documented and tested (e.g. space-separated scopes, `key=value` pairs).
- [ ] `GET /auth/config` returns `200` JSON with the issuer, client id, scopes and extra parameters when `APP_JWT_ISSUER` is set, and `404` (an `ErrorBody`, ADR 0022) when it isn't; tests cover both.
- [ ] `/auth/config` does not appear in `frontend/openapi.json` or among the MCP tools; `make check-spec` still passes with no snapshot change.
- [ ] `AppConfig::validate` refuses to start in prod with the SPA embedded (`embed-frontend`) when `APP_JWT_ISSUER` is set and `APP_OIDC_CLIENT_ID` isn't, with a message naming both variables, matching the dev-secret refusal; a test covers it, and dev/test or a build without the SPA embedded still starts.
- [ ] `.env.example` documents the three new settings and how to register the SPA at the provider as a public client (PKCE, no secret) with redirect URI `<origin>/auth/callback`.
- [ ] `make check-changed` passes.
