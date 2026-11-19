---
status: needs-info
---

# Environments can run on AWS Lambda and scale to zero

## Problem

Each customer gets a single-tenant environment. The app runs as long-lived processes: `server`, `worker` and `migrate` from one image (`Dockerfile`), with Postgres, and Valkey once a module broadcasts (ADR 0039). So an idle environment costs as much as a busy one. Lambda could scale it to zero, but six parts hold state between requests:

- **The worker** polls every `worker_poll_ms`, 1 s by default (`crates/server/src/bin/worker.rs`). `worker --drain` is specified (Spec 0004), not built.
- **MCP sessions** live in one process's memory, forgotten after 5 idle minutes (`crates/common/web/src/mcp/sessions.rs`, ADR 0034).
- **The `Bus`** is Redis pub/sub for SSE or WebSocket streams (`crates/common/bus/src/lib.rs`, ADR 0039). No module broadcasts and no route streams yet: live topics (ticket 0024) were set aside.
- **A pool per process,** 10 connections by default (`db_max_connections`, `crates/server/src/config.rs`), so a burst of Lambdas can exhaust Postgres.
- **JWKS** is discovered and fetched before serving (`JwtVerifier::from_discovery`, `crates/common/web/src/auth.rs`, called in `crates/server/src/main.rs`), so on every cold start.
- **Grants** are read before serving, then re-polled every 5 s by a spawned task (`access::POLL_EVERY`, `crates/platform/access/core/src/application/rules.rs`). A frozen Lambda runs no task.

And the database, not compute, sets what an idle environment costs (below).

## Solution overview

- **One codebase, two hosts.** The same crates build the container image and a Lambda package; the build and config pick the host. The container host keeps every behaviour it has.
- **The server is one Lambda,** through Poem's Lambda adapter ([`poem-lambda`](https://crates.io/crates/poem-lambda)), outside any VPC, behind a function URL (question 7).
- **The worker is a Lambda running `worker --drain`,** woken after a commit and when the next job is due, with no fixed tick (question 5).
- **Postgres is Neon,** one project per environment, scaling to zero after 5 idle minutes (question 2), reached with a pool of 2 per Lambda and reserved concurrency (question 4).
- **MCP is stateless** (question 3); **live screens poll** (question 6).
- **Each environment is its own AWS account** in one AWS Organization (question 8).
- **Built later, not now** (question 1).

## Behaviour

### Cost per environment per month

us-east-1, on-demand, checked 2026-11-19; arithmetic at 730 h a month. Idle: no requests. Light: about 2 h of use on 22 working days, 20k requests. Busy: about 10 h a day on 22 days, 500k requests. Each assumes 2 GB of data. A serverless database stays awake 5 minutes after the last request, so it is awake about 66 h a month when light and 242 h when busy, averaging 0.5 CU or ACU when light and 1 when busy.

| Setup (database: question 2) | Idle | Light | Busy |
|---|---|---|---|
| A. Fargate (server and worker, 0.25 vCPU / 0.5 GB arm, $7.21 each) + RDS `db.t4g.micro` ($11.68 + $2.30 storage) + ALB ($16.43) + 4 public IPv4 ($14.60) | ~$60 | ~$60 | ~$60–70 |
| B. One Lightsail 1 GB instance running compose, Postgres included | $7 | $7 | $12 (2 GB) |
| C. Lambda + Aurora Serverless v2 paused at 0 ACU, Aurora public over TLS | ~$4 | ~$8 | ~$35 |
| D. Lambda + Neon Launch | ~$1 | ~$4 | ~$26 |

- **Containers:** [Fargate](https://aws.amazon.com/fargate/pricing/), [RDS for PostgreSQL](https://aws.amazon.com/rds/postgresql/pricing/), [ALB](https://aws.amazon.com/elasticloadbalancing/pricing/), [public IPv4](https://aws.amazon.com/vpc/pricing/), [Lightsail](https://aws.amazon.com/lightsail/pricing/) bundles of $7 (1 GB) and $12 (2 GB), each with its IPv4 address.

- **Lambda itself is nearly free here:** $0.20 per million requests and $0.0000133 per GB-s on arm64; busy is about 25k GB-s, under $1 even without the free tier of 1M requests and 400k GB-s ([pricing](https://aws.amazon.com/lambda/pricing/)). Start-up time is billed too, since 1 Aug 2025 ([AWS blog](https://aws.amazon.com/blogs/compute/aws-lambda-standardizes-billing-for-init-phase)).
- **Aurora Serverless v2:** $0.12 per ACU-hour, $0.10 per GB-month, $0.20 per million I/Os, about 10M when busy ([pricing](https://aws.amazon.com/rds/aurora/pricing/)). Paused it costs only storage, plus $3.65 for the public IPv4 its instance holds. It resumes in about 15 s, 30 s or more after a day paused ([auto-pause](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html)).
- **Neon Launch:** $0.106 per compute-hour, $0.35 per GB-month, no monthly minimum, scale to zero after 5 minutes. It resumes "within a few hundred milliseconds" ([pricing](https://neon.com/pricing), [scale to zero](https://neon.com/docs/introduction/scale-to-zero)).
- **The fixed costs that dominate,** each per environment if not designed out:
  - a NAT gateway: $32.85 plus $0.045 per GB, once a Lambda sits in a VPC and must reach the internet (the identity provider's JWKS);
  - an interface VPC endpoint: $7.30 per AZ;
  - a public IPv4 address: $3.65 ([VPC pricing](https://aws.amazon.com/vpc/pricing/));
  - RDS Proxy: $0.015 per ACU-hour, and it **stops Aurora pausing** ([pricing](https://aws.amazon.com/rds/proxy/pricing/), auto-pause page above);
  - WAF: $5 per web ACL, $1 per rule, $0.60 per million requests ([pricing](https://aws.amazon.com/waf/pricing/)). Shared at the edge, not per environment (question 7).
- **Cold resume** is paid by the first request after an idle spell: a Lambda's start, plus 15–30 s for Aurora, or a few hundred ms for Neon.

### The server

- A `lambda` feature builds `server` with `poem-lambda` instead of a TCP listener; the routes and middleware stay `build_app`'s.
- **Pool:** `db_max_connections` is 2 on Lambda. Reserved concurrency caps the server's Lambdas at 20, so connections stay under 45 (question 4); Neon allows 112 direct connections at its smallest compute ([pooling](https://neon.com/docs/connect/connection-pooling)).
- **JWKS** is fetched on the first token on Lambda, then cached as now, so a cold start serving the SPA pays nothing for it; the container host still fetches before serving.
- **Grants** are refreshed by an invocation that finds them older than `POLL_EVERY`, instead of by a task; a role change still takes effect within seconds.
- The SPA is still served from the binary (ADR 0005).

### The worker

- `worker --drain` (Spec 0004) runs as its own Lambda with reserved concurrency 1, so drains never overlap.
- **Woken after a commit:** a Unit of Work that wrote a job, event or deferred job invokes the worker asynchronously once it commits (question 5).
- **Woken when due:** each drain ends by setting one [EventBridge Scheduler](https://aws.amazon.com/eventbridge/pricing/) one-time schedule at the earliest pending `run_at`, replacing the last; 14M a month are free. Nothing due, nothing set, so the database can sleep (question 5).
- **A daily safety tick** drains anything a lost nudge left (question 5).
- The container `worker` is unchanged.

### MCP and live screens

- `/mcp` on Lambda is stateless: no `Mcp-Session-Id`, each request carries and is checked by its own token (question 3). Server-initiated messages aren't offered; no tool sends any today.
- A Lambda environment has no `Bus` and no stream: screens refetch on focus and on an interval (question 6). A module that broadcasts can't be deployed to Lambda, and the build says so.

### Deploying

- `migrate` runs as a one-off invocation before the new version takes traffic.
- One AWS account per environment, created through AWS Organizations, with an AWS Budgets alert each (question 8). Organizations allows 10 accounts by default, raised on request up to 50,000 ([quotas](https://docs.aws.amazon.com/organizations/latest/userguide/orgs_reference_limits.html)).

### Docs

- A new ADR records the Lambda host; ADR 0034 (stateless MCP) and ADR 0039 (no bus on Lambda) are amended. `docs/platform/glossary.md` gains **Host**.

## Out of scope

- Infrastructure as code and the deploy pipeline beyond the build's artifacts.
- Live push on Lambda (question 6), and modules that broadcast there.
- Multi-tenant environments, and other clouds' function hosts.

## Open risks

- **Neon puts customer data with a second vendor,** outside the customer's AWS account (question 2, question 8).
- **Lambda limits:** 6 MB requests and responses, 15-minute invocations ([limits](https://docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html)). Large uploads and exports need S3.
- **`poem-lambda`** is far less used than Poem's TCP server; it's pinned in pairs with Poem (ADR 0026).
- **A runaway agent** bills Lambda per call; reserved concurrency and the Budgets alert are the backstop.

## Needs Human Input (factory)

Prices are those in "Cost per environment per month", checked 2026-11-19.

1. **Build this now, later, or not at all?**
   - *Why it matters:* it's sizeable work (a second host, a stateless MCP, the worker's wake-ups), and Spec 0004's `--drain` must land first.
   - *Options:* **Now**: saves about $35–60 per environment a month against Fargate and RDS (A against D), but only $3–6 against one Lightsail instance (B) when idle or light, and costs about $14 more when busy. **Later**, once there are enough environments for the saving to repay the work, with B as the cheap host meanwhile. **Never**: stay on containers, accepting B's single instance or A's cost.
   - *Recommendation:* later. B is already close to D for small customers; Lambda pays off at scale or once customers need managed, highly available databases.
   - *Depends on it:* whether this Spec is ticketed at all.
2. **Which Postgres per environment?**
   - *Why it matters:* it's most of the cost, and the one part that decides how slow the first request after idle is.
   - *Options:* **Neon**: cheapest (D), resumes in milliseconds, public over TLS so no NAT; data held by a second vendor. **Aurora Serverless v2 with auto-pause**: data in the customer's AWS account; about $3–9 a month more (C), and a 15–30 s first request after 5 idle minutes; needs Aurora public or a NAT ($32.85). **RDS `db.t4g.micro` always on**: about $14 a month whatever the use; no pause.
   - *Recommendation:* Neon: cheapest, and the resume is invisible. Aurora where a customer requires their data in their own account.
   - *Depends on it:* "Cost", "Pool", the vendor risk.
3. **MCP stateless, or sessions kept in Postgres?**
   - *Why it matters:* consecutive requests of one MCP session can reach different Lambdas, which don't share memory.
   - *Options:* **Stateless**, which [MCP's transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports) allows: little work, every tool still works; loses server-to-client messages (none are used). **Sessions in Postgres**: a table and a lookup per request; keeps the protocol's sessions, and wakes the database for each.
   - *Recommendation:* stateless; nothing today needs a session.
   - *Depends on it:* "MCP and live screens", the ADR 0034 amendment.
4. **How are database connections pooled?**
   - *Why it matters:* each Lambda holds its own pool, so 100 at once could open 1,000 connections.
   - *Options:* **A pool of 2 per Lambda and reserved concurrency**: free, but caps throughput at 20 concurrent requests. **Neon's PgBouncer**, 10,000 clients: free; needs sqlx's prepared statements checked against transaction pooling. **RDS Proxy** (Aurora only): a fixed cost, and Aurora never pauses.
   - *Recommendation:* a pool of 2 and reserved concurrency: free and safe at single-tenant volumes.
   - *Depends on it:* "Pool".
5. **How is the worker woken?**
   - *Why it matters:* a fixed tick of a minute keeps the database awake all month, which undoes scaling to zero.
   - *Options:* **A fixed tick**: simplest; the database never sleeps. **Nudge after commit plus a one-time schedule at the next due job, with a daily safety tick**: a little code in the Unit of Work and the drain; the database sleeps whenever nothing is due. **Nudge only**: deferred and recurring work (Spec 0004) wouldn't run until something else commits.
   - *Recommendation:* nudge plus next-due schedule.
   - *Depends on it:* "The worker".
6. **How do screens learn of others' changes?**
   - *Why it matters:* a Lambda can't hold a subscription to the `Bus`.
   - *Options:* **Polling only**: no work, since no screen streams today; a screen is up to its interval stale. **API Gateway WebSockets**: $0.25 per million connection-minutes plus $1 per million messages ([pricing](https://aws.amazon.com/api-gateway/pricing/)), a connections table, a new path for every broadcast. **Response streaming**: a Lambda billed for every second a stream stays open, and still no bus.
   - *Recommendation:* polling only, until a module needs live push.
   - *Depends on it:* "MCP and live screens", the ADR 0039 amendment.
7. **What sits in front of the server?**
   - *Why it matters:* it sets the request timeout, the custom domain and where WAF can attach.
   - *Options:* **A function URL**: no charge beyond Lambda's, public only ([docs](https://docs.aws.amazon.com/lambda/latest/dg/urls-configuration.html)); a custom domain or WAF needs CloudFront in front. **API Gateway HTTP API**: $1 per million requests ([pricing](https://aws.amazon.com/api-gateway/pricing/)), custom domains, a 30 s timeout ([quotas](https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-quotas.html)). **An ALB**: $16.43 a month fixed, which undoes the saving.
   - *Recommendation:* a function URL, behind one CloudFront and WAF shared by every environment if needed.
   - *Depends on it:* "Solution overview", the WAF cost.
8. **One AWS account per tenant, or shared accounts?**
   - *Why it matters:* it sets the blast radius of a mistake or a breach, and how costs are seen per customer.
   - *Options:* **An account per tenant**: hard isolation, cost per account in the bill, a Budgets alert each; needs automated account creation and a raised account quota. **Shared accounts, resources per tenant**: simpler to start; IAM and tagging do the isolating, and one mistake can reach every tenant.
   - *Recommendation:* an account per tenant: with no fixed cost per environment, accounts cost nothing extra.
   - *Depends on it:* "Deploying".
