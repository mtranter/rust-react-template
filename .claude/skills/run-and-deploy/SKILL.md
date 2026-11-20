---
name: run-and-deploy
description: Deploy or operate the app in production: the one image and its server, worker and migrate processes, running migrations, scaling or cron-driving the worker (`--drain`), clock requirements, load balancing MCP sessions, and the environment a deployment needs. Use when the user asks how to deploy, run, scale or operate the app, or changes a binary's startup.
---

# Running and deploying

Extends `crates/AGENTS.md`; if this file and the code disagree, the code wins: fix this file in the same change. For sign-in configuration, use the `configure-auth-provider` skill. Every variable is described in `.env.example`; `crates/server/src/config.rs` says which refuse to start.

## One image, three processes (ADR 0005)

One binary serves the API and the embedded SPA (built with `embed-frontend`; without it, `/` answers with a page pointing at Vite). The image runs:

- `server`: the API, the SPA and `/mcp`.
- `worker`: its own deployment, scaled by replicas sharing the queue, or run by an external timer with `--drain`.
- `migrate`: one-off, a pre-start task of each deployment.
- `seed`: built with `SEED=true`, refuses `APP_ENV=prod`.

Each binary exports traces and metrics only when `OTEL_*` is set (the `add-telemetry` skill).

## Migrations (ADR 0021)

Migrations run explicitly (`make migrate`, a pre-start task in a deployment), never on server startup. Each module migrates its own schema; then `migrate` registers topics, subscriptions and schedules, retires the Schedules this build doesn't declare, and applies the default grants this build adds (`server::apply_default_grants`). Because subscriptions are registered here and not on worker start, old and new builds relay alike during a rolling deploy. A migration file applied anywhere is immutable, its comments included (sqlx checksums it).

## The worker

- Without flags, `worker` polls every `APP_WORKER_POLL_MS`, relaying, backfilling, scheduling, running jobs and compacting.
- `worker --drain [--max-seconds N]` (`Worker::drain`) runs the poll loop's pass (`run_until_idle`: relaying, backfilling, scheduling and running; then compaction if due) until nothing is due by its clock, then exits `0`, so cron or a container scheduler can drive it with no state outside Postgres. `--max-seconds` caps it: once spent it exits `0`, dropping what is in flight as a stopped worker would (the handler's writes roll back; its job runs again after the 5-minute lease, the attempt counted). Failed jobs are retried and dead-lettered as usual and never change the exit code; it exits non-zero only when it can't work at all: the database unreachable, a subscription or schedule unregistered, a schedule's expression that doesn't parse, or its clock refused, each logged with its cause. `--max-seconds` without `--drain` is a usage error (exit `2`).
- **A worker refuses to start when its clock is more than a minute from the database's** (`Worker::check_clock`), and skips compaction while it is: workers judge leases, due times and retention by their own clocks (ADR 0020), so they must agree to well within the 5-minute lease. The database is only the reference; a wrong database clock stops every worker, which is accepted.
- Dead letters have no admin surface yet: inspect and repair them with SQL on `messaging.job`.

## Server instances

- **MCP sessions live in the process that opened them** (ADR 0034): behind several server instances, the load balancer routes by the `Mcp-Session-Id` header. `APP_PUBLIC_URL` sets the URL MCP clients are told to sign in for; unset, each request's `Host` and `X-Forwarded-*` say.
- Each instance rebuilds its access rule set from the database every 5 s, so a grant or assignment reaches every instance within that.
- A module that broadcasts makes the server and worker refuse to start without `APP_REDIS_URL`; give them the same one (ElastiCache Serverless for Valkey over TLS in production: `rediss://<user>:<password>@<endpoint>:6379`).
- `APP_ENV=prod` refuses the built-in dev `APP_JWT_SECRET` when no issuer is set.

## The assistant (Spec 0012)

The server's `assistant` is off unless `APP_AGENT_MODEL` is set: the server starts as usual, its status operation says off, and the SPA hides its page. With it set:

| Variable | Default | What |
|---|---|---|
| `APP_AGENT_MODEL` | none (off) | The Bedrock model or inference-profile id, e.g. an Anthropic Claude Sonnet profile. |
| `APP_AGENT_MAX_TOOL_CALLS` | `10` | Tool calls per turn. |
| `APP_AGENT_MAX_OUTPUT_TOKENS` | `4096` | Tokens the model may answer with per turn; `0` refuses to start. |
| `APP_AGENT_HISTORY_MESSAGES` | `50` | A conversation's latest messages sent to the model; `0` refuses to start. |

- **AWS region and credentials** come from the AWS SDK's default chain (`AWS_REGION`, `AWS_PROFILE`, an instance or task role, ...), loaded on the first call to the model, so a missing credential shows then, not at start-up.
- **IAM**: the server's role needs `bedrock:InvokeModelWithResponseStream` and `bedrock:InvokeModel` on that model (and, for an inference profile, on the profile and the models it routes to). The worker and `migrate` need none.
- **Data residency**: whatever the tools return for a user, their data included, is sent to the configured model in its region. An app with data-residency or contractual limits picks its region and model (or a geographic inference profile, `eu.` ...) to suit, or leaves the assistant off.
