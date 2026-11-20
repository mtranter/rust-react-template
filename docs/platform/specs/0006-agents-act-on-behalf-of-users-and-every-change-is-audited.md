---
status: needs-info
---

# Agents act on behalf of users, and every change is audited

## Problem

An agent calls the app with the user's own token (ADR 0034), so the app can't tell the agent from the user it acts for:

- a Principal is a user's id and roles, with no actor (`crates/common/authz/src/principal.rs`);
- no policy can treat agents differently: a condition compares only a row's attributes (`R.attr`), with `==`, `!=` and `in` (`crates/common/authz/src/condition.rs`, ADR 0040);
- an agent acting for no one (a nightly reconciler, an ops agent) has no sanctioned way in.

Nor is there any record of who changed what:

- the event envelope carries no actor (`crates/common/events/src/envelope.rs`);
- the outbox keeps only each key's latest event once its topic's retention, 7 days, has passed (ADR 0033);
- roots have no `created_by` or `updated_by`; only `access.role` has a `created_by` (`crates/platform/access/core/migrations/0002_roles.sql`).

The options, standards and prices behind this Spec are in `docs/platform/research/0001-agent-identity-and-audit.md`, each outside fact with its source, checked 2026-11-19.

## Solution overview

- **A Principal carries an actor.** Beside whom it acts for (the token's `sub`), a Principal knows which agent, if any, is acting. The app reads it from the token: the [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html) `act` claim when the identity provider issues one, otherwise the token's client ([RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html) `client_id`, or `azp`) when that client isn't one of the app's own. So it works with any provider, and better with one that does token exchange (question 1).
- **An agent acting for no one** signs in with client credentials. Its token's subject is its own client, recorded by `access` as a Subject and given roles by Assignment, like a user (ADR 0041).
- **Policies can tell agents apart.** The condition language gains principal-only comparisons and integer ordering, both still compiling to SQL (ADR 0040).
- **Every change is audited.** The publisher writes an audit record in the same Unit of Work as each event it publishes. Every state change publishes one (ADR 0012), so no change commits unrecorded. Audit records are never compacted; the worker deletes them only once they're older than the retention (question 2).
- **Admins read the audit log through `ops`** (Spec 0005), over REST and so MCP.

## Behaviour

### Who is acting

- `Principal::User` gains `actor: Option<Actor>`: the acting agent's id and, from nested `act` claims, the chain of agents behind it (RFC 8693 §4.1). `None` when the user acts themselves. `Principal::System` is unchanged.
- The default resolver sets the actor from, in order:
  1. the token's `act` claim, its outermost `sub` being the current actor;
  2. otherwise `client_id` (or `azp`) when it isn't listed in `APP_FIRST_PARTY_CLIENTS`, which defaults to `APP_OIDC_CLIENT_ID` (Spec 0002).
- A client-credentials token's subject is its client, so an agent acting for no one is a Principal whose actor is itself.
- `/mcp` refuses a token whose `aud` isn't this app, as the [MCP authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization) requires; `APP_JWT_AUDIENCE` becomes required when `APP_JWT_ISSUER` is set. The app never forwards a token it was given.
- `access`' Subjects record whether each one has been seen as an agent, so admins can tell agents from people when assigning roles.

### Policies

- A condition may compare the principal alone: `P.actor == null`, `P.actor != null`, `P.actor in [...]`, and `P.attr.<b>` against a literal. It resolves to true or false when the plan is made, so it never reaches SQL.
- `R.attr.<a>` may be compared with `<`, `<=`, `>` and `>=` to an integer literal or `P.attr.<b>`. It compiles to the same comparison in SQL, and a missing value makes it false, as `==` does.
- So "agents may not delete" is a DENY on `delete` with `P.actor != null`, and "agents approve only under 1000" is a DENY on `approve` with `all` of `P.actor != null` and `R.attr.amount >= 1000`.
- `access`' baseline rules deny every agent its admin actions: granting, revoking, assigning, unassigning, and creating and deleting roles (question 5). An agent can't widen what it, or anyone, may do.
- ADR 0040 is amended to describe the extended language.

### Audit records

- The publisher writes one record per published event, in the event's transaction, to `audit.record`, owned by `app_db` beside `messaging.*`. A record holds (question 3):
  - when, by the instance's clock, and the event's id;
  - the topic, key, version and event type, so "which root, which change" is exact;
  - whom it was for (the Principal's `sub`, or `system`) and the actor and its chain, if any;
  - the request id, the operation id, and the trace id when there is one (Spec 0003);
  - for a reaction, the id of the event that caused it, so a change made by the worker leads back to the person or agent behind it.
- The same who and actor go into the event's envelope, as the CloudEvents [`authcontext`](https://github.com/cloudevents/spec/blob/main/cloudevents/extensions/authcontext.md) extension (`authtype`, `authid`), plus an `actor` attribute.
- Only changes are recorded: refused and read-only requests are not (question 4).
- Records are append-only: a trigger refuses `UPDATE` and `DELETE` except by the retention purge (question 6).

### Retention

- `APP_AUDIT_RETENTION`, by default 400 days (13 months), is the deployment's choice, not the code's: how long records are kept is a compliance fact about the environment, unlike a topic's retention (ADR 0033), which is a fact about its consumers (question 2).
- The worker deletes older records in batches, on the same schedule as it compacts the outbox, as its own maintenance (ADR 0015, 0033).

### Reading

- `ops` serves `list_audit_records` and `get_audit_record`, filtered by subject, actor, topic and key, event type and time range, newest first, paged as other lists (ADR 0018). Its default grants give them to `admin`, who can grant them to others, such as `operator`, at runtime.
- `ops`' screen lists them, filtered the same way.

### Docs

- `docs/platform/glossary.md` gains **Actor** and **Audit Record**; ADR 0040 and 0025 are amended and a new ADR records the audit log.

## Out of scope

- The app issuing tokens or doing token exchange itself: the identity provider does that (ADR 0025).
- Narrowing a delegated agent to scopes the user consented to; the actor DENY rules are the narrowing for now.
- Auditing the database outside the app (migrations, manual SQL), and exporting records to a SIEM or archive.
- `created_by` and `updated_by` on roots: the audit record answers who changed a root.

## Open risks

- **`ops` doesn't exist yet** (Spec 0005 is a draft). Whichever lands second adds the audit operations.
- **Client id as the actor** depends on the provider putting `client_id` or `azp` in access tokens, which not every provider does for every flow (question 1).
- **Personal data.** Records keep subject ids for the retention. Erasing a person means replacing their id with a pseudonym in the audit log, which is not built here.
- **Volume.** A record per event roughly doubles each write's rows. That's cheap at this template's scale; a busy module may want an index or partitioning by month.

## Needs Human Input (factory)

Prices and support were checked on 2026-11-19; sources are in the research doc, sections 2 and 4.

1. **Which identity provider will environments use?**
   - *Why it matters:* only the provider can issue tokens naming an agent acting for a user (token exchange, RFC 8693's `act`), and it is a cost paid per environment. Without `act`, the app still recognises an agent by its client, but it can't see a chain of agents.
   - *Options:*
     - **Amazon Cognito Essentials.** Issues `act` since 2026-11-12 ([announcement](https://aws.amazon.com/about-aws/whats-new/2026/11/cognito-token-exchange/)). About $0.015 per monthly user, the first 10,000 per AWS account free; $0.00225 per client-credentials token request, for agents acting for no one ([pricing](https://aws.amazon.com/cognito/pricing/)). Nothing to run. Risk: the feature is a week old.
     - **Keycloak, self-hosted.** Free and mature, but `act` is a preview feature ([docs](https://www.keycloak.org/securing-apps/token-exchange)), and each environment needs a server (roughly $15–60/month, estimated) to patch and back up.
     - **Auth0.** Token exchange from B2C Professional, $240/month for up to 12 environments ([pricing](https://auth0.com/pricing)); its free plan works with the client fallback only.
     - **Entra External ID.** Free to 50,000 monthly users, but its on-behalf-of flow is its own and has no `act` ([docs](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-on-behalf-of-flow)): client fallback only.
   - *Recommendation:* Cognito, since environments are expected on AWS (`Review-2026-19-11.md`, 4.8): cheapest with nothing to operate. Confirm token exchange on one user pool first. Okta (from $1,500/year) and Zitadel (token exchange experimental) are in the research doc.
   - *Depends on it:* "Who is acting" (whether `act` is ever present), and the open risk on client ids.

2. **How long are audit records kept?**
   - *Why it matters:* it decides storage cost, what an auditor or a customer's dispute can be answered from, and how long personal data is held.
   - *Options:*
     - **90 days.** Least data held; too short for PCI DSS (12 months) or a SOC 2 audit period.
     - **13 months (400 days).** Covers a 12-month PCI or SOC 2 window with a month's margin. About $0.12/month per million 1 KB records on RDS (unverified price).
     - **7 years.** Covers HIPAA-style documentation periods; seven times the data, and harder to defend under GDPR.
     - **Forever.** Simplest; grows without bound, and conflicts with GDPR's storage limitation.
   - *Recommendation:* 13 months as the default of `APP_AUDIT_RETENTION`, overridable per environment, so a regulated customer can get longer without a release.
   - *Depends on it:* "Retention".

3. **Does a record hold the changed root's state, or only who, what and when?**
   - *Why it matters:* with the state, the log can show what a value was before someone changed it; without it, it shows that they changed it, and the outbox has the details for 7 days only.
   - *Options:*
     - **Who, what, when only.** Small rows; personal data is only subject ids.
     - **Plus the state after.** The event already carries it (ADR 0012), so it's free to write. Rows are 1–5 KB, and erasing a person means rewriting their records.
     - **Plus a diff from the previous state.** Most readable, but needs the previous state on every write.
   - *Recommendation:* who, what, when only: enough to answer "who changed this", and it keeps retention and erasure simple. State can be added later; records already written can't gain it.
   - *Depends on it:* "Audit records", the personal-data risk.

4. **What is audited: changes only, or also refusals and reads?**
   - *Why it matters:* refusals show an agent probing what it may not do; reads show who saw what. Each adds a write to requests that write nothing today.
   - *Options:*
     - **Changes only.** Free given the publisher; nothing on reads.
     - **Changes and refusals** (`403`, `404` from a policy). One insert per refusal, outside any Unit of Work, so best effort.
     - **Changes, refusals and reads.** A write on every request; the log grows by orders of magnitude.
   - *Recommendation:* changes only. Refusals already reach the logs, and traces once Spec 0003 lands; recording them can be added when a customer needs it.
   - *Depends on it:* "Audit records".

5. **What may agents not do, by default, in every app?**
   - *Why it matters:* an agent injected with hostile instructions can do anything its user may. A baseline rule bounds that in every app built from the template.
   - *Options:*
     - **Nothing by default.** Each app writes its own rules.
     - **Agents may not administer `access`** (grant, revoke, assign, unassign, roles). One rule in `access`; an agent can't widen anyone's rights.
     - **That, and no deletes.** Safer, but every module's `delete` stops working over MCP, against ADR 0034's "MCP gets whatever REST gets".
   - *Recommendation:* agents may not administer `access`. It closes privilege escalation, costs one rule, and leaves the rest to each app.
   - *Depends on it:* "Policies".

6. **Must the audit log resist tampering?**
   - *Why it matters:* the app's database role can rewrite any table, so a compromised app, or an operator, could edit history.
   - *Options:*
     - **An ordinary table.** No cost; trust rests on database access.
     - **Append-only by trigger:** `UPDATE` and `DELETE` refused except by the retention purge. A migration and a function; stops mistakes and casual edits, not a database superuser.
     - **Also a hash chain, or a copy to S3 Object Lock.** Proves tampering; more code, a serialised insert or an export job, and storage outside Postgres.
   - *Recommendation:* append-only by trigger: cheap, and stops accidental and casual edits. A hash chain can follow if a customer asks for proof.
   - *Depends on it:* "Audit records".


### From Human.

I agree to all the recommendations.

## Needs Human Input (factory)

TicketCreator failed: planned Ticket t3 depends on "0072", which is neither an earlier planned Ticket nor a known Ticket Id
