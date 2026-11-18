# Auth is one JWT Bearer scheme, verified only

The template shows how a guarded route works and goes no further: a single JWT Bearer `SecurityScheme` in `app-web`, guarding one route in `users` (`me`). The scheme flows into the Spec and so into the Generated Client. There is no user store and no token issuance backend; everything beyond verification is a seam for the project built from the template.

`JwtVerifier` has two modes, chosen from config:

- **HS256** (development and tests): a shared secret, `APP_JWT_SECRET`. Self-contained, and can issue tokens, which examples and tests use.
- **JWKS** (production): set `APP_JWT_ISSUER` to an OIDC issuer, and tokens are verified against its published keys, discovered from `{issuer}/.well-known/openid-configuration`. Verify-only. Keys are cached by `kid` and refetched when a token presents an unknown one, which handles rotation. `APP_JWT_AUDIENCE` optionally requires a matching `aud`.

## Considered options

- **A built-in user store with sessions or token issuance.** Every real project picks its own identity provider; a template's would be thrown away.

## Consequences

- A failed auth renders as the shared `ErrorBody` (ADR 0022).
- Journeys sign in with real HS256 tokens (ADR 0029).
- Authentication says who the caller is; what they may do is [ADR 0040](0040-code-declares-what-can-be-granted-admins-grant-it-at-runtime-and-lists-filter-in-sql.md): the scheme's checker resolves the token to a Principal, and each Module's operations take its `Caller`, which passes the Module's gate. So every operation is guarded but those in `server::PUBLIC`, not one route.
