# Agent identity, on-behalf-of tokens and an audit log

Research for Spec 0006. Options and recommendations, not a survey. Outside facts were checked on **2026-11-19** at the URL given. "Unverified" marks a fact no official page confirmed.

## 1. Acting on someone's behalf: the standards

**Token exchange (RFC 8693, <https://www.rfc-editor.org/rfc/rfc8693.html>).**

- An agent sends the authorization server the user's token (`subject_token`) and optionally its own (`actor_token`). It gets back a new token for this app.
- `sub` stays the user. An `act` claim names the agent (§4.1). Nesting is the chain: `{"sub": "alice", "act": {"sub": "agent-a", "act": {"sub": "agent-b"}}}` means `agent-a` acts now, for `alice`, having been handed the work by `agent-b`. Only the outermost `act` is the current actor. The RFC says prior actors are informational, never for access control.
- `may_act` (§4.4), in the user's token, names who may become its actor. The *authorization server* checks it when exchanging, not the app.

**Client credentials** (OAuth 2.0, RFC 6749 §4.4) is an agent acting for no one. The token's subject is the agent's own client.

**The token's client** (RFC 9068 §2.2, <https://www.rfc-editor.org/rfc/rfc9068.html>) is `client_id`, a required claim in a JWT access token. Many providers write it as `azp`. An agent that signs a user in through MCP's flow gets a token whose `sub` is the user and whose client is the agent. So **the app can tell an agent from the SPA without token exchange**, by the client, as long as each agent is its own client. Token exchange adds the chain, and a way in for agents that hold no user session of their own.

**MCP's authorization spec** (2025-11-25, <https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization>):

- A server MUST serve Protected Resource Metadata (RFC 9728). `/mcp` already does: `crates/common/web/src/mcp/discovery.rs` names `APP_JWT_ISSUER` as its authorization server.
- Clients MUST send `resource` (RFC 8707). The server MUST check that `aud` names it, and MUST NOT accept tokens meant for others, or pass on a token it was given. Today `APP_JWT_AUDIENCE` is optional (`crates/server/src/config.rs`), so `/mcp` would take any of the issuer's tokens.
- Client ID Metadata Documents are now SHOULD and Dynamic Client Registration is MAY. Client credentials appears only as something a client MAY try.
- Token exchange is not part of MCP. It happens between the agent and the provider, and the app sees only its result. So nothing in `/mcp` changes beyond reading `act` and the client, and requiring `aud`.

**Recommendation.** Read the actor from `act` when present, otherwise from the client when it isn't one of the app's own. Require `aud`. The design then works with every provider below, and is better with one that issues `act`.

## 2. Identity providers, for many single-tenant environments

| Provider | Token exchange (RFC 8693) / `act` | Client credentials | Cost per environment |
|---|---|---|---|
| **Keycloak** (self-hosted) | Standard exchange since 26.2, internal tokens only ([release](https://www.keycloak.org/2025/04/keycloak-2620-released)). `act` only via *Token Exchange Delegation*, a **preview** feature, off by default ([docs](https://www.keycloak.org/securing-apps/token-exchange)). | Service accounts, unlimited | No licence. Hosting a container and its database: roughly $15–60/month (unverified estimate), or one shared server with a realm per environment, coupling them. Upgrades and backups are yours. |
| **Auth0** | *Custom Token Exchange*, RFC 8693-shaped, with an actor set by an Action; `act` in the issued token unverified ([docs](https://auth0.com/docs/authenticate/custom-token-exchange)). B2C/B2B Professional and Enterprise only. | 1,000 tokens/month on Free and Essentials, 5,000 on Professional ([pricing](https://auth0.com/pricing)) | Free: 25,000 MAU, 1 tenant. Token exchange needs B2C Professional, $240/month for up to 12 tenants. |
| **Okta** | RFC 8693 on custom authorization servers, `act` nested up to 5 actors ([guide](https://developer.okta.com/docs/guides/set-up-token-exchange/main/)) | Yes | Custom authorization servers are a paid add-on; Customer Identity starts at $3,000/month, Workforce has a $1,500/year minimum ([pricing](https://www.okta.com/pricing/)). |
| **Entra ID / External ID** | Its own *On-Behalf-Of* flow (`jwt-bearer` grant), not RFC 8693, and no `act` claim; user principals only ([docs](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-on-behalf-of-flow)). The app sees the agent only as the token's client. | Yes | External ID: first 50,000 MAU free ([pricing](https://www.microsoft.com/en-us/security/pricing/microsoft-entra-external-id/)); OBO in External ID tenants unverified. |
| **Amazon Cognito** | RFC 8693 announced 2026-11-12, adding `act` for an actor token; Essentials and Plus tiers ([announcement](https://aws.amazon.com/about-aws/whats-new/2026/11/cognito-token-exchange/), [`GetDerivedToken`](https://docs.aws.amazon.com/cognito-user-identity-pools/latest/APIReference/API_GetDerivedToken.html)). Whether the `/oauth2/token` endpoint takes the grant is unverified ([grants](https://docs.aws.amazon.com/cognito/latest/developerguide/federation-endpoints-oauth-grants.html)). | $0.00225 per token request, no free tier | Essentials $0.015/MAU, 10,000 MAU free per AWS account ([pricing](https://aws.amazon.com/cognito/pricing/)). Hundreds of users per environment is cents to a few dollars a month. |
| **Zitadel** | Token exchange is a "first iteration", user-id subject tokens experimental; `act` carries the actor ([docs](https://zitadel.com/docs/guides/integrate/token-exchange)) | Service users included | Cloud Free: 100 daily active users; Pro $100/month ([pricing](https://zitadel.com/pricing)); per-environment billing unverified. Self-hosted free under AGPL-3.0 ([licence](https://zitadel.com/license-faq)). |

**Recommendation: Amazon Cognito Essentials**, if environments run on AWS, as the review assumes (`Review-2026-19-11.md`, 4.8, and ADR 0039's ElastiCache). It costs least per environment with nothing to operate, and now issues `act`. Its token exchange is a week old: confirm the flow end to end on one user pool before relying on `act`. The client-id fallback (section 1) covers it meanwhile. Next best is Keycloak, which is free but is a server per environment to run.

## 3. How a Principal carries an actor

Today `Principal::User { id, roles, attr }` (`crates/common/authz/src/principal.rs`). Its roles are `user` plus `access`' Assignments (ADR 0041), resolved from `sub` by `access::sign_ins`. `Claims` has no `act` or client (`crates/common/web/src/auth.rs`).

- **Add `actor: Option<Actor>`** to `Principal::User`: the current actor's id and the chain behind it. The user's roles are unchanged: a delegated agent may do at most what its user may, minus whatever rules deny agents. An agent acting for no one is a Subject in `access` with its own Assignments, and its own actor.
- **Extend the policy language minimally.** A condition today is `R.attr.<a>` compared with `==` or `!=` to `P.id`, `P.attr.<b>` or a literal, or `R.attr.<a> in [...]` (`crates/common/authz/src/condition.rs`, ADR 0040). Two additions:
  1. **Principal-only comparisons**: `P.actor == null`, `!= null`, `in [...]`, and `P.attr.<b>` against a literal. `Condition::resolve` already substitutes the principal, so these become `Filter::True` or `Filter::False` before any SQL is built. Nothing new reaches SQL.
  2. **Integer ordering**: `<`, `<=`, `>`, `>=` between `R.attr.<a>` and an integer literal or `P.attr.<b>`. That is four new `Filter` variants, each one SQL comparison, and `NULL` is false as with `==`.
- "Agents may not delete": `effect: EFFECT_DENY, actions: [delete], roles: ["*"], condition: P.actor != null`. "Agents approve only under 1000": a DENY on `approve` with `all` of `P.actor != null` and `R.attr.amount >= 1000`. Both are developers' rules: ADR 0040 lets admins grant, never deny.
- **Rejected:** full CEL, or arithmetic. Neither compiles to SQL in general, which ADR 0040 rejected for the same reason.

## 4. Audit log options

Every state change publishes an event in its Unit of Work (ADR 0012), through `EventPublisher::publish` (`crates/common/persistence/src/publishing.rs`), which writes `messaging.outbox` (`crates/common/db/src/publishing.rs`).

| | A. Table, same Unit of Work | B. Built from events | C. Postgres triggers / pgaudit |
|---|---|---|---|
| **How** | The publisher inserts an `audit.record` beside each outbox row, in the same transaction | The actor goes in the envelope; a subscriber copies each event into an audit table | Triggers copy each row change into a table ([supa_audit](https://github.com/supabase/supa_audit), [pgMemento](https://pgxn.org/dist/pgmemento/doc/pgmemento-docs-0.7.4/Transaction_log.html)); pgaudit logs statements to the server log ([README](https://github.com/pgaudit/pgaudit/blob/main/README.md)) |
| **A row records** | who, actor chain, topic/key/version/event type, request, operation and trace ids; optionally the state after | the same, but written later | triggers: the old and new row, and the app user only if passed per transaction with `set_config(..., true)` ([docs](https://www.postgresql.org/docs/current/functions-admin.html)); pgaudit: the database role and SQL only, never the app's user |
| **Complete?** | Yes: no change commits without its record | Eventually: a dead-lettered audit handler leaves a gap until retried, and an `earliest` backfill after compaction (ADR 0033) cannot recover lost history | Triggers: yes, but rows are tables, not roots or events; pgaudit: yes, but at SQL level |
| **Kept** | Its own retention, purged by the worker | Its own retention | Triggers: as A. pgaudit: whatever the log sink keeps (CloudWatch, Cloud Logging) |
| **Read by `ops`** (Spec 0005) | A list query in `app_db`, filtered and paged (ADR 0018) | The same | Triggers: the same, but the rows are table-shaped. pgaudit: not from the app at all |
| **Cost** | One more insert per event; small code in `app_db` | A subscription, queue and handler per topic; a delay | supa_audit warns of lower write throughput (archived 2025-02-16); a migration per table; pgaudit is free on RDS, Azure and Cloud SQL, but its logs are billed by the sink |

At RDS gp3's roughly $0.115/GB-month (unverified: [AWS calculator](https://calculator.aws/#/addService/RDSPostgreSQL)), a million 1 KB records cost about $0.12 a month to keep.

**Retention** has no single legal answer. PCI DSS 10.5.1 asks for 12 months, 3 of them immediately available ([summary](https://withpci.pages.dev/requirements/10/10.5/10.5.1)). SOC 2 sets no period, but 12 months is the norm ([Linford](https://linfordco.com/blog/data-retention-policy-soc-2/)). HIPAA's 6 years covers documentation, not logs as such ([45 CFR 164.316](https://www.law.cornell.edu/cfr/text/45/164.316)). GDPR asks that personal data be kept no longer than needed ([Art. 5(1)(e)](https://gdpr-info.eu/art-5-gdpr/)).

**The envelope** can carry the actor through CloudEvents' `authcontext` extension: `authtype` (`user`, `service_account`, `system`…), `authid`, `authclaims` ([spec](https://github.com/cloudevents/spec/blob/main/cloudevents/extensions/authcontext.md)). It has no field for an actor acting for someone else, so that one needs an extension attribute of our own.

**Recommendation: A**, with the actor also in the envelope. It is the only option that is complete by construction and knows the app's user. It reuses the one place every change already passes through. B can be added later for outside sinks, from the same envelope.
