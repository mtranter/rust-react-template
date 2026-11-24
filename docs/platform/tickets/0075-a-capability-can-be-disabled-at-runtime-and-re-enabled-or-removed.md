---
id: 0075
name: A Capability can be disabled at runtime, keeping its code and data, then re-enabled or removed
status: needs-triage
priority: P2
size: M
blocked_by: [0074]
blocks: []
source: conversation on 2026-11-24 about data lifecycle across Capabilities; recorded in ADR 0047 as later work
---

# A Capability can be disabled at runtime, keeping its code and data, then re-enabled or removed

Lower priority: not needed yet. Recorded so that ticket 0074 leaves room for it, not to be scheduled.

## Description

Soft-delete a Capability: stop serving it, but keep its code, its migrations and its data, so that it can be turned back on. ADR 0047 records the decision. Removing a Capability for good is the later step.

## Evidence (as of 2026-11-24)

- **The worker doesn't stop work for subscriptions nobody serves.** The relay copies events into the queue of every subscription in `messaging.subscription`, served or not. `Worker::check_subscriptions` only warns of "a live subscription this worker doesn't serve" (`crates/common/db/src/worker.rs:234`). A Capability that is unregistered but still subscribed fills `messaging.job` forever.
- **A retired queue can't come back.** Its name is bound forever and never reused (`crates/AGENTS.md`), so retiring is removal, not a pause.
- **Grants already go dormant safely.** `access` skips a stored grant naming what no loaded policy has, and logs it once per revision (`crates/platform/access/core/src/application/rules.rs:41`).
- **Leaving a Capability out of the spec breaks its SPA code.** Its operations leave `openapi.json`, so its generated client functions disappear and its feature no longer type-checks.

## Proposed approach

- **A `disabled` set in the composition** (`server::Registered`). A disabled Capability is still compiled, migrated and in the spec. Its endpoints aren't mounted (they answer `404`), MCP leaves out its tools, and `get_my_permissions` leaves out its resources.
- **Subscriptions.** A paused state on `messaging.subscription` that the relay skips. Disabling pauses the Capability's subscriptions and cancels its pending deferred jobs.
- **Retention.** Its retention Schedules keep running while it is disabled, so its personal data is purged on time. Each Schedule declares whether it is one.
- **Re-enabling.** It resumes its reactions from then, never running the backlog, and starts a new Generation for each projection, from `earliest`.
- **`make check-modules`** refuses a disabled Capability whose `-api` an enabled one uses.
- **Removing.** Delete the code, retire its subscriptions by migration, and revoke its grants.

## Acceptance criteria

- A disabled Capability's operations answer `404` and its tools are missing from `/mcp`. The shell doesn't show it. Its data, grants and code remain, and nothing accumulates in `messaging.job` for it.
- Its retention purges still run.
- When re-enabled, it serves again with the same grants. Its projections are rebuilt and nothing published while it was disabled triggers a reaction.

## Open questions

- Which of a Capability's Schedules count as retention, and is it the Schedule's own declaration that says so?
- Should disabling be a deploy-time setting (an environment variable) or something an admin toggles?
