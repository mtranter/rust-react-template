---
id: 0072
name: "A Principal carries the acting agent, read from the token's act claim or its non-first-party client"
status: in-progress
spec: docs/platform/specs/0006-agents-act-on-behalf-of-users-and-every-change-is-audited.md
blocked_by: []
blocks: []
---

# A Principal carries the acting agent, read from the token's act claim or its non-first-party client

Give `Principal::User` an `actor: Option<Actor>` (the acting agent's id and the chain of agents behind it), and have the default resolver (`impl PrincipalResolver for Authorizer` in `crates/common/web/src/auth.rs`, also used by `access`' `RecordingSignIns`) set it from the token. `Claims` gains `act` (nested, RFC 8693), `client_id` and `azp`. A new `APP_FIRST_PARTY_CLIENTS` setting lists the app's own clients and defaults to `APP_OIDC_CLIENT_ID`. A client-credentials token (subject is its own client) becomes a Principal whose actor is itself, recorded by `access` as a Subject and assignable roles like a user. `Principal::System` is unchanged.

## Acceptance criteria
- [ ] `app_authz` has an `Actor` type (id plus chain of outer-to-inner agents) and `Principal::User { actor: Option<Actor>, .. }`; `Principal::user(..)` still builds a Principal with no actor, and there is a way to build one with an actor for tests
- [ ] `Claims` deserialises `act` (including nested `act` inside `act`), `client_id` and `azp`, all optional, and `Claims` can sign them for dev/test tokens
- [ ] The resolver sets the actor from the outermost `act.sub`, with the nested `act`s as its chain; otherwise from `client_id` (or `azp` when `client_id` is absent) when that client is not in `APP_FIRST_PARTY_CLIENTS`; otherwise `None`
- [ ] `APP_FIRST_PARTY_CLIENTS` is read by the server config (comma- or space-separated), defaults to `APP_OIDC_CLIENT_ID`, and is documented in `.env.example`
- [ ] Tests cover: a user's own token through the SPA's client (no actor); a token with `act` (actor and chain); a token from another client with no `act` (actor is the client); a client-credentials token whose `sub` is its client (actor is itself), which `access` records as a Subject and to which a role can be assigned and takes effect
- [ ] `docs/platform/glossary.md` defines **Actor**; ADR 0025 is amended to say how the actor is read and that the app never forwards a token it was given; the `endpoint-auth` and `configure-auth-provider` skills mention the actor, `APP_FIRST_PARTY_CLIENTS` and client-credentials agents
- [ ] `make check-changed` passes
