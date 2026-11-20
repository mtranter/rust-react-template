# Auth is one JWT Bearer scheme, verified only

The template shows how a guarded route works and goes no further: a single JWT Bearer `SecurityScheme` in `app-web`, guarding one route in `users` (`me`). The scheme flows into the Spec and so into the Generated Client. There is no user store and no token issuance backend; everything beyond verification is a seam for the project built from the template.

`JwtVerifier` has two modes, chosen from config:

- **HS256** (development and tests): a shared secret, `APP_JWT_SECRET`. Self-contained, and can issue tokens, which examples and tests use.
- **JWKS** (production): set `APP_JWT_ISSUER` to an OIDC issuer, and tokens are verified against its published keys, discovered from `{issuer}/.well-known/openid-configuration`. Verify-only. Keys are cached by `kid` and refetched when a token presents an unknown one, which handles rotation. `APP_JWT_AUDIENCE` optionally requires a matching `aud`.

## Who is acting (amended, Spec 0006)

An agent calling for a user presents a token for that user, so beside whom it acts for (`sub`) a Principal carries the Actor, the agent acting, read from the token by the default resolver in this order:

1. An [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html) `act` claim, which a provider doing token exchange issues: its outermost `sub` is the Actor, the nested `act`s the chain of agents behind it (§4.1).
2. Otherwise the token's client, [RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html)'s `client_id` or else `azp`, when it isn't listed in `APP_FIRST_PARTY_CLIENTS` (by default `APP_OIDC_CLIENT_ID`): a token another client obtained is that client acting.
3. Otherwise none: the user acts themselves.

So any provider works, and one doing token exchange shows the whole chain. An agent acting for no one uses client credentials: its token's subject is its own client, so it is a Principal whose Actor is itself, recorded by `access` as a Subject and assigned Roles like a user (ADR 0041).

**The app never forwards a token it was given.** It neither calls another service with a caller's token nor exchanges one: issuing and exchanging tokens is the provider's.

## Considered options

- **A built-in user store with sessions or token issuance.** Every real project picks its own identity provider; a template's would be thrown away.
- **The Actor from `act` alone.** Exact, but only some providers do token exchange; the client fallback names an agent with any of them, without its chain.

## Consequences

- A failed auth renders as the shared `ErrorBody` (ADR 0022).
- Journeys sign in with real HS256 tokens (ADR 0029).
- Authentication says who the caller is; what they may do is [ADR 0040](0040-code-declares-what-can-be-granted-admins-grant-it-at-runtime-and-lists-filter-in-sql.md): the scheme's checker resolves the token to a Principal, and each Module's operations take its `Caller`, which passes the Module's gate. So every operation is guarded but those in `server::PUBLIC`, not one route.
