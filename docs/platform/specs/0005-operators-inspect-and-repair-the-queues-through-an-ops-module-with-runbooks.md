---
status: draft
---

# Operators inspect and repair the queues through an ops module, with runbooks

## Problem

When something goes wrong after a request has succeeded, nobody can see it or fix it without raw SQL on `messaging.*`:

- a handler failing until it's dead-lettered;
- the relay stalling;
- a backfill stuck;
- no worker running at all.

`Worker::dead_letters` exists for tests only. There is no list of what is queued, how old it is, or what state each subscription and backfill is in. There is no record of what an error `code` means operationally or what to do about it.

Ops agents (a later step) need to diagnose and repair over MCP, as everything else is done. Humans brought in by escalation need the same view.

## Solution overview

A platform Module, `ops`, in `crates/platform/ops`, beside `access`. `make init` keeps it. It's guarded by a new role, `operator`, through `access`, and exposed over REST and so MCP (ADR 0034):

- **Read:** dead letters, queue status, relay and backfill status, compaction status, workers.
- **Repair:** retry or discard dead letters, one at a time or by filter.
- **Runbooks:** one markdown runbook per error `code` and per failure mode, kept in the repo, embedded in the binary and served by `ops`, so an agent loads the one it needs.

A minimal screen gives escalated humans the same view.

## Behaviour

### Access

- `ops`' default grants give `operator` and `admin` its gate and every action. `migrate` creates the `operator` role (ADR 0041).
- Handlers run against `messaging.*` through `app_db`. `ops` owns no queue data, so it reads and repairs what the worker owns, through the worker's own types in `app_db`.

### Operations

Each has an `operation_id` and a summary written for an agent.

- `list_dead_letters`: filter by queue, event type and key; sorted by when it died; paged as other lists are (ADR 0018).
- `get_dead_letter`:
  - payload, `last_error`, attempts and timestamps;
  - its trace link when the event carries trace context (Spec 0003);
  - whether it is **superseded**: a later version of the same key has already run on that queue.
- `retry_dead_letter`:
  - due now, attempts reset;
  - refused `409` when superseded, because the worker doesn't make later versions wait behind a dead one (`crates/common/db/src/worker.rs`), so running it would apply a stale version;
  - refused `409` when it isn't dead.
- `retry_dead_letters`: by queue, and optionally event type. It retries every matching dead letter that isn't superseded and returns how many it retried and how many it skipped.
- `discard_dead_letter`: removes it.
- `get_queue_status`, per queue:
  - due, waiting and running;
  - the oldest due job's age;
  - dead letters;
  - its subscription's state (registered or retired);
  - backfill progress.
- `get_relay_status`: undispatched rows, the oldest one's age, and when the relay last dispatched.
- `get_compaction_status`, per topic: retention, when it was last compacted, and rows held.
- `list_workers`: each running worker's host, version, start time and last heartbeat.
  - Workers record a heartbeat in a new `messaging.worker` table every 15 seconds.
  - A worker unseen for 2 minutes is listed as absent.
- `list_runbooks` and `get_runbook`: served from the embedded files.

### Audit

- Each retry and discard is an event on `ops`' topic, carrying the principal who did it and what was retried or discarded. That's the record until there is an audit log (Spec 0006).

### Runbooks

- They live in `docs/runbooks/`, one file per error `code` and per failure mode:
  - a dead letter;
  - relay stalled;
  - backfill stuck;
  - worker absent;
  - clock refused.
- Each says, in this order: the symptom, likely causes, which `ops` operation to check, the fix, and when to escalate to a human.
- A composed test fails when an `ErrorCode` has no runbook, as `the_instructions_say_what_every_refusal_status_means` does for statuses.
- The server's MCP instructions tell agents to call `get_runbook` with an error's `code`.
- They're served as operations, not MCP resources, because poem-mcpserver 0.3.1 always answers `resources/list` with nothing and has no `resources/read`. Moving them to resources is part of the MCP tool hints work (Spec 0008).

### Screen

- `features/ops`, shown to `operator` and `admin` only:
  - queue status;
  - dead letters, with retry and discard, behind confirmation;
  - workers.
- Tested against MSW.

### Docs

- `crates/AGENTS.md`: `ops` is platform, and a new error `code` needs a runbook.
- `GLOSSARY.md` gains **Runbook** and **Operator**.

## Out of scope

- Metrics and alert rules (Spec 0003 exports metrics; alerting is a later Spec).
- Editing a dead letter's payload, replaying arbitrary events, and re-running a backfill.
- Schedules, until scheduled work (Spec 0004) lands. Whichever of the two lands second lists them here.
- An audit log beyond `ops`' own events (Spec 0006).

## Open risks

- **What a later version means.** "Superseded" assumes every handler cares about version order. A handler that doesn't, such as one sending a notification, may want a forced retry. That's left out until a case needs it.
- **Scale.** `get_queue_status` counts rows in `messaging.job`, which may need an index or estimates at volume.
