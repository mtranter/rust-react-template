---
name: configure-auth-provider
description: Configure an OIDC identity provider (Auth0, Entra ID, Okta, Keycloak, Cognito, Google, ...) so a deployment of this app signs people in and verifies their tokens, and give it its first admin. Use when the user wants to set up, switch or debug sign-in, an issuer, a client id, scopes, audience, redirect URIs, or names missing from the Users table.
---

# Configure an auth provider

The app takes any **OIDC** provider (not plain OAuth 2): the server verifies each request's JWT access token against the issuer's published keys, and the SPA signs in at the issuer itself with the authorization code flow and PKCE (ADR 0025, 0042). There is no backend-for-frontend, no client secret, no token issuance and no user store. Configuration is environment variables only.

Sources of truth, read them before answering if anything below looks stale (the code wins; fix this file in the same change):

- `.env.example`: every variable, with its comment.
- `crates/server/src/config.rs`: parsing, and `validate_serving`'s refusals to start.
- `crates/server/src/sign_in.rs`: `GET /auth/config`, what the SPA is told.
- `crates/common/web/src/auth.rs`: `Claims` (and `Claims::actor`, the agent acting) and `JwtVerifier` (JWKS discovery, `aud` check).
- `crates/common/web/src/mcp/discovery.rs`: the Protected Resource Metadata MCP clients read.
- `frontend/src/lib/sign-in/`: the SPA's side.

## 1. Check the provider fits

Ask the user which provider, then confirm it:

- publishes `{issuer}/.well-known/openid-configuration` with a `jwks_uri`;
- issues **JWT access tokens** for the app (some issue opaque ones unless told which API they're for, e.g. Auth0 without `audience`);
- supports a public SPA client with authorization code + PKCE.

`APP_JWT_ISSUER` must equal the token's `iss` exactly, trailing slash included (Auth0's has one, Entra v2's doesn't).

## 2. Register the SPA as a client at the provider

- Type: single-page application / public client, authorization code with PKCE, **no client secret**.
- Redirect URI: `<origin>/auth/callback` (e.g. `https://app.example.com/auth/callback`).
- Post-logout redirect URI: `<origin>/auth/signed-out`.
- Allowed web origin / CORS origin: `<origin>`.
- Refresh tokens: allow them (rotation fine). Without them the SPA renews silently in a hidden iframe with `prompt=none`, which needs the provider's session cookie; browsers blocking third-party cookies then make the user sign in again.
- If the provider separates APIs from clients (Auth0 APIs, Entra "Expose an API"), register the API and note its identifier: it becomes the audience.

## 3. Set the server's environment

| Variable | Required | Meaning |
|---|---|---|
| `APP_JWT_ISSUER` | yes | The issuer. Set, tokens are verified by JWKS and `APP_JWT_SECRET` is unused. |
| `APP_OIDC_CLIENT_ID` | yes in prod | The SPA client's id. A prod server with the SPA embedded refuses to start without it. |
| `APP_JWT_AUDIENCE` | recommended | Requires a matching `aud`; the API identifier. |
| `APP_OIDC_SCOPES` | optional | Space-separated, quoted, beyond `openid profile email`. |
| `APP_OIDC_EXTRA_PARAMS` | optional | Space-separated `key=value` added to the authorization request; the server refuses to start on one that isn't `key=value`. |
| `APP_PUBLIC_URL` | optional | Where clients reach the server, named in MCP sign-in metadata. Unset, taken from `Host`/`X-Forwarded-*`. |
| `APP_BOOTSTRAP_ADMINS` | yes, at first | Comma-separated `sub`s that always hold `admin` (step 5). |
| `APP_FIRST_PARTY_CLIENTS` | optional | The app's own client ids, comma- or space-separated; defaults to `APP_OIDC_CLIENT_ID`. A token any other client obtained names that client as the agent acting (step 5a). |

`APP_ENV=prod` also refuses to start with no issuer and the built-in dev `APP_JWT_SECRET`.

Per provider (check the provider's current docs; these drift):

- **Auth0**: issuer `https://<tenant>.<region>.auth0.com/` (trailing slash). `APP_OIDC_EXTRA_PARAMS="audience=<API identifier>"` or the access token is opaque; same value in `APP_JWT_AUDIENCE`. `offline_access` in scopes for refresh tokens.
- **Entra ID**: issuer `https://login.microsoftonline.com/<tenant-id>/v2.0`. Expose an API on the app registration and set `APP_OIDC_SCOPES="api://<app-id>/.default offline_access"`; set `accessTokenAcceptedVersion: 2` in the manifest so `iss` matches the v2 issuer. Audience is the app's client id (or `api://<app-id>`, check the token).
- **Okta**: use a custom authorization server's issuer (`https://<org>.okta.com/oauth2/default`); the org server's access tokens aren't meant for your API. Audience `api://default` unless changed.
- **Keycloak**: issuer `https://<host>/realms/<realm>`; add an audience mapper so `aud` carries the client or API.
- **Cognito**: issuer `https://cognito-idp.<region>.amazonaws.com/<pool-id>`; Cognito access tokens carry `client_id`, not `aud`, so leave `APP_JWT_AUDIENCE` unset.
- **Google**: its access tokens are opaque, so it can't be used directly; put a broker (Auth0, Keycloak, Cognito) in front.

## 4. Names and emails in the Users table

`access` records a subject's `name` and `email` from the **access token** (`Claims` has `sub`, `name`, `email`; `given_name`/`family_name` aren't read). Many providers leave profile claims out of access tokens; then `/access` shows the email or the bare `sub`. Fix it at the provider: an Auth0 Action adding `name`/`email` to the access token, Entra optional claims, an Okta or Keycloak claim mapper. The header's signed-in name comes from the ID token and is unaffected.

## 5. The first admin

No one holds `admin` on a fresh deployment. Have the operator sign in once, read their `sub` (from the provider's user record, or the access token at jwt.io), set `APP_BOOTSTRAP_ADMINS=<sub>` and restart. That subject always holds `admin` and can't be unassigned it; they then assign roles to others at `/access`. Leaving it set is safe; while it is empty, the last assigned `admin` can't be unassigned.

## 5a. Agents

A Principal carries the agent acting for its subject, its **Actor** (ADR 0025): the token's RFC 8693 `act` claim when the provider does token exchange (Cognito, Keycloak's preview, Auth0's paid plans), with any nested `act`s as the chain behind it; otherwise the token's `client_id` (else `azp`) when it isn't in `APP_FIRST_PARTY_CLIENTS`. So:

- List every client of the app's own (the SPA, a mobile app) in `APP_FIRST_PARTY_CLIENTS`, or their users' requests are taken for an agent's. Unset, only `APP_OIDC_CLIENT_ID` is.
- Register each agent (an MCP client, an assistant) as a client of its own, and check its access tokens carry `client_id` or `azp`: not every provider puts them in every flow, and without one or `act`, the agent's requests look like its user's.
- An agent acting for no one (a nightly reconciler) uses the **client-credentials** grant. Its token's `sub` is its client (Auth0 makes it `<client-id>@clients`), which `access` records as a Subject once it calls; assign it roles at `/access` like a user. It needs the same `aud` as users' tokens.
- The app never forwards a token: an agent gets its own from the provider.

## 6. Verify

1. `curl <origin>/auth/config` returns the issuer, client id, scopes and extra params (404 means `APP_JWT_ISSUER` is unset).
2. Sign in through the SPA; it should land back where it started.
3. A `401` on every API call: compare the access token's `iss` and `aud` with the config, and confirm it is a JWT (three dot-separated parts), not opaque, and that its `kid` is in the issuer's JWKS. The server doesn't log why it refused a token, so decode one and compare by hand.
4. A `403` on every guarded call: signed in fine, but no grant for the caller's roles; see the `endpoint-auth` skill.
5. Open `/access` as the bootstrap admin.

Locally, `make dev` signs every `/api` request in as the seeded Alice through the Vite proxy and needs none of this. To exercise a real sign-in locally, `make e2e` runs the whole flow against `crates/common/fake-oidc` (`scripts/e2e.sh` shows the variables it sets).
