---
status: needs-info
---

# Requests are limited per caller, and in size and time

## Problem

Nothing bounds what one caller can make the app do:

- **No body limit:** `build_app` and `api` (`crates/server/src/app.rs`) add CORS, tracing, request ids and idempotency, but no `SizeLimit`; nothing in `crates/common/web/src/` sets one.
- **No timeout:** no request is cut off, the server closes no idle connection (`Server::new(..)` without `idle_timeout`, `crates/server/src/main.rs`), and the pool sets no `acquire_timeout` or `statement_timeout` of its own (`app_db::connect`, `crates/common/db/src/lib.rs`).
- **No rate limit,** per caller or at all. An agent calls with a user's token (ADR 0034), so a runaway agent loop reaches every operation, over REST or as MCP tools, as fast as it can send.
- **JWKS refresh is uncapped:** a token whose `kid` isn't cached makes `Jwks::verify` fetch the key set again, every time (`crates/common/web/src/auth.rs`), with no HTTP timeout (`reqwest::Client::new()`). An unsafe request verifies its token twice, once for idempotency (`verified_caller`) and once for its `Caller`, so one forged token costs two fetches. Anyone can send one.
- **MCP sessions are unbounded in number:** each `initialize` adds one, kept 5 idle minutes (`crates/common/web/src/mcp/sessions.rs`).

Each environment is single-tenant (Spec 0010). So the likely harm is not a flood from strangers but one signed-in caller, most often an agent, doing too much: running up the database, the bill, or other users' latency.

Two plans were put forward. **The owner's:** handle DDoS at the infrastructure level, and possibly put API Gateway in front of everything with a custom authorizer that also rate-limits, for quotas per tenant. **The review's:** WAF and CloudFront at the edge, shared by every environment; API Gateway, if at all, only for routing and coarse throttling; and limits per principal and per actor inside the app, which alone knows who is acting. Question 1 weighs them.

## Solution overview

- **Floods stop at the edge (question 1).** One CloudFront distribution and WAF web ACL, shared across environments, with AWS's managed rules and a rate-based rule per client IP. It's infrastructure, outside this repo; the app documents what it expects of it.
- **Callers are limited in the app (question 1).** A middleware inside `api` (`crates/server/src/app.rs`) counts each request against its caller, so REST and every MCP tool call, which reach the same endpoint in-process, count alike.
- **Every request is bounded in size and time (question 2).** A body limit and a timeout, set by config, with a larger limit only where an operation declares it.
- **Counters are in memory, per instance (question 4).** No table, no new dependency.
- **The JWKS refresh is capped here (question 6).**
- **No quotas over days or months (question 5).**

## Behaviour

### Size and time (question 2)

- Every request body is at most `APP_MAX_BODY_BYTES`, by default 1 MiB. Beyond it the app answers `413` with the `ErrorBody` (ADR 0022), code `payload_too_large`, without reading the rest. A body sent without `Content-Length` is cut off at the same limit, not refused.
- An operation needing more declares its own limit beside its handler; none does today.
- A request taking longer than `APP_REQUEST_TIMEOUT`, by default 25 s, is abandoned: its transaction rolls back and it answers `503`, code `unavailable`. 25 s is under the 30 s that CloudFront and API Gateway wait for an origin, so a caller gets the app's error body, with its request id, rather than the edge's bare `504`.
- Each connection's Postgres `statement_timeout` is the same, so the database stops the query too, rather than finishing work no one waits for.
- The server closes a connection idle for 60 s.
- `/mcp` has the same limits; an MCP tool call is one request.

### Limits per caller (question 3)

- A caller is the token's subject, and, once Spec 0006 gives Principals an actor, the subject and actor together: each agent acting for a user has its own allowance, apart from the user's own. Until then, the subject alone.
- Each caller has a token bucket per kind of request, by default:
  - **all requests:** 600 a minute, bursting to 100;
  - **unsafe requests** (POST, PUT, PATCH, DELETE): 120 a minute, bursting to 30;
  - **for an agent acting for a user** (once Spec 0006 lands): half of each, so a runaway agent hits its limit before the user does.
- Requests with no valid token are limited by the edge's rule per IP, not here; the app counts them all as one anonymous caller with the "all requests" limit, as idempotency gives them one shared scope (`crates/common/web/src/idempotency.rs`).
- Each limit is set by config (`APP_RATE_LIMIT_*`), and `0` turns it off (question 7).
- A refused request answers `429` ([RFC 6585 §4](https://www.rfc-editor.org/rfc/rfc6585#section-4), checked 2026-11-19), code `rate_limited`, with `Retry-After` in seconds. It is refused before idempotency, so a retry with the same `Idempotency-Key` runs as if new. The MCP instructions (`crates/server/src/tools.rs`) say to wait `Retry-After` before retrying.
- `Principal::System` (the worker) is never limited: it isn't an HTTP caller.
- Each refusal is logged with the caller and operation, at most once a minute per caller, so a runaway agent shows in the logs.

### Where counters live (question 4)

- In memory in each server instance, with an idle caller's buckets dropped after 10 minutes. With N instances behind a load balancer a caller can get up to N times its limit; a single-tenant environment runs one or two.
- The counter sits behind a trait in `app-web`, so Postgres or Valkey can replace it without touching the middleware, as a Lambda host would need (Spec 0010).

### JWKS (question 6)

- An unknown `kid` refreshes the key set at most once every 30 s, and concurrent requests share one fetch; within the cooldown an unknown `kid` is `401` at once. The cooldown is the default of [`jose`'s `createRemoteJWKSet`](https://github.com/panva/jose/blob/main/docs/jwks/remote/interfaces/RemoteJWKSetOptions.md) (checked 2026-11-19).
- Discovery and key fetches time out after 5 s.
- A subject holds at most 10 MCP sessions; opening an eleventh drops its least recently used.

### Docs

- A new ADR records where limits are enforced and why. ADR 0022 gains `payload_too_large` and `rate_limited`. `docs/platform/glossary.md` gains **Rate Limit**. `crates/AGENTS.md` says how an operation declares a larger body.

## Out of scope

- The edge itself: CloudFront, WAF and their rules are infrastructure as code, which this repo doesn't hold (Spec 0010).
- Quotas per day or month, and billing by use (question 5).
- Limits that admins change at runtime, or that differ by role (question 7).
- Limiting the worker, and outbound calls to other services.
- `RateLimit` response headers: still an [IETF draft](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/) (checked 2026-11-19).

## Open risks

- **Poem's `SizeLimit` refuses a body without `Content-Length` with `411`** (`poem` 3.1.12, `src/middleware/size_limit.rs`), and has no timeout middleware; both are likely small middleware of the app's own.
- **Abandoning a request mid-transaction** relies on dropping its future rolling back; a handler with a side effect outside the database (none today: ADR 0011) could leave it half done.
- **Default limits are guesses.** Too low, they break a legitimate bulk import by an agent; the logs of refusals are how to tell.
- **Per-instance counters** are only as strict as the number of instances; a deployment that scales out widely needs shared counters (question 4).

## Needs Human Input (factory)

Prices are us-east-1; every outside source below was checked 2026-11-19.

1. **Where are limits enforced?**
   - *Why it matters:* it decides what can be limited (an IP, a token, an agent, an operation) and what each request costs.
   - *Options:*
     - **Edge only** (WAF on CloudFront): $5 a web ACL, $1 a rule and $0.60 per million requests a month ([WAF pricing](https://aws.amazon.com/waf/pricing/)), shared across environments; or a CloudFront flat-rate plan with WAF included, $0 (Free) or $15 (Pro) a month ([CloudFront pricing](https://aws.amazon.com/cloudfront/pricing/)). Stops floods; a rate rule can key on a header ([rate-based rules](https://docs.aws.amazon.com/waf/latest/developerguide/waf-rule-statement-type-rate-based-high-level-settings.html)), but WAF can't verify a token, so a caller varying it escapes, and it can't see the actor or the operation. A runaway agent with a valid token goes unchecked.
     - **API Gateway with a custom authorizer that rate-limits** (the owner's plan): $3.50 per million requests for a REST API, which has usage plans, or $1 for an HTTP API, which hasn't ([pricing](https://aws.amazon.com/api-gateway/pricing/)), plus a Lambda per uncached token. Rejects bad tokens before compute and can keep quotas across tenants in one place. But it duplicates the app's token checks in a second codebase; adds the authorizer's latency to every uncached call; every MCP tool call is `POST /mcp`, so it can't tell a read from a delete; and AWS says usage plans' throttling and quotas are best-effort, not hard limits ([usage plans](https://docs.aws.amazon.com/apigateway/latest/developerguide/api-gateway-api-usage-plans.html)).
     - **API Gateway throttling plus limits in the app:** the gateway's per-route throttle as a coarse ceiling, the app for callers. Costs the gateway's per-request price for a ceiling the edge's rate rule already gives.
     - **Edge plus the app** (the review's): the edge's cost above, and the app's middleware. The app sees the verified subject, the actor and the operation, at no cost per request; the edge keeps floods and unauthenticated traffic off the server.
   - *Recommendation:* edge plus the app. In a single-tenant app the risk is a signed-in caller doing too much, which only the app can see; the edge already handles floods, as the owner planned; and a quota per tenant is the environment's whole bill, which an AWS Budgets alert (Spec 0010) watches without a gateway.
   - *Depends on it:* "Solution overview", "Limits per caller", the new ADR.
2. **What default body size and timeout?**
   - *Why it matters:* too small breaks legitimate requests; too large lets one caller tie up memory or a connection.
   - *Options:* **1 MiB and 25 s:** fits every operation today, all JSON, and answers before the edge's 30 s ([CloudFront origin timeout](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/DownloadDistValuesOrigin.html), [API Gateway quotas](https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-quotas.html)). **256 KiB and 10 s:** tighter, may break a large list import. **6 MB and 60 s:** Lambda's own payload ceiling ([limits](https://docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html)); a request outlives the edge's timeout, which would need raising.
   - *Recommendation:* 1 MiB and 25 s, with larger limits declared per operation. Uploads belong in S3, not request bodies.
   - *Depends on it:* "Size and time".
3. **What default limits per caller, and per agent?**
   - *Why it matters:* they decide whether a runaway agent is stopped in seconds or minutes, and whether a legitimate one is ever refused.
   - *Options:* **None by default,** only the mechanism: nothing breaks, nothing is protected until someone configures it. **Generous** (600 a minute, 120 of them unsafe; agents half): far above a person clicking, stops a loop within a minute. **Strict** (60 a minute, 20 unsafe): stops a loop fast, and likely refuses an agent's legitimate batch work.
   - *Recommendation:* generous, with agents at half. A person rarely sends 10 requests a second; a loop does.
   - *Depends on it:* "Limits per caller".
4. **Where do the counters live?**
   - *Why it matters:* it decides how exact a limit is across instances, and what each request costs.
   - *Options:* **In memory per instance:** free, no latency; a limit is per instance, and useless on Lambda, where each instance is short-lived. **Postgres:** exact; one upsert per request, contended for a busy caller, and it wakes a sleeping database (Spec 0010). **Valkey:** exact and fast; makes Valkey required, which ADR 0039 requires only once a module broadcasts.
   - *Recommendation:* in memory, behind a trait, until the app runs on more instances or on Lambda.
   - *Depends on it:* "Where counters live", the Open risk.
5. **Are there quotas over days or months?**
   - *Why it matters:* a quota ("10,000 writes a day per agent") must survive restarts, so it needs Postgres, unlike a rate limit.
   - *Options:* **None:** nothing to build; the Budgets alert watches cost. **Per agent per day in Postgres:** a table and a write per request; caps an agent's total work. **Per tenant per month at API Gateway:** only with option 2 or 3 of question 1; best-effort, as above.
   - *Recommendation:* none until a customer is billed by use.
   - *Depends on it:* "Solution overview", "Out of scope".
6. **Does the JWKS refresh cap belong here?**
   - *Why it matters:* it's the one limit anyone can trigger without signing in, and it costs a request to the identity provider each time.
   - *Options:* **Here:** small, and the same concern: a caller making the server do unbounded work. **Its own ticket, now:** ships sooner, apart from decisions it doesn't need. **Not at all:** the provider's own rate limits answer for it, and turn into `500`s for everyone.
   - *Recommendation:* here, as a 30 s cooldown with one shared fetch, unless this Spec waits on other answers; then its own ticket.
   - *Depends on it:* "JWKS".
7. **Are limits config, or data admins change?**
   - *Why it matters:* an admin may want to give one agent more, or throttle one now, without a deploy.
   - *Options:* **Config:** env vars, changed by a redeploy; nothing to build. **Data in `access`:** limits per role or subject, changed at runtime like grants (ADR 0040); a table, operations and a screen.
   - *Recommendation:* config, until someone needs one caller treated differently.
   - *Depends on it:* "Limits per caller", "Out of scope".
