---
id: 0007
name: Add a transaction seam, a transactional outbox and a background worker
status: todo
priority: P1
size: L
blocked_by: [0002, 0006]
blocks: []
source: architecture review 2026-10-31, finding 1
---

# Add a transaction seam, a transactional outbox and a background worker

## Description
The template's biggest gap for real apps. Every repository runs directly on the
pool, and there is no way to share a transaction. Nothing can:
- save two aggregates atomically;
- save and then reliably trigger a side effect (email, audit, notification);
- let one module react to another's change ("user deleted → archive their notes");
- run a background or scheduled job.

D6's load→act→save can't be extended to any of these. Without an example, each
module invents its own pattern (passing `PgPool` into services, firing side
effects after `save` with no atomicity, or writing through another module's `-api`).

## Evidence (as of 2026-10-31)
- **Repositories run on the pool.** They hold `PgPool` and run every query on
  `&self.pool` (`users/core/src/db/repository.rs`, `notes/core/src/db/repository.rs`).
- **No transaction helper** in `crates/common/db/src/lib.rs`.
- **Check-then-save isn't atomic.** `NoteService::create` checks author existence via
  `users-api` and then saves, as two independent calls.
- **Cross-module calls are query-only.** `users_api::UserDirectory` is synchronous and
  query-only, and there is no event, outbox or job concept anywhere.

## Proposed approach
1. **Unit of work.** A `UnitOfWork`/`Tx` abstraction in `app-persistence` (pure trait),
   implemented in `app-db` over `sqlx::Transaction`. Services open one and pass it to
   repository methods (or get transaction-scoped repositories from it). The clippy
   tripwire must still keep `PgPool` out of `application/`.
2. **Outbox.** A per-module `outbox` table, created by the shared runner or per-module
   migration. The repository `save` writes domain events in the same transaction.
   Events are versioned types published in `<module>-api`, with plain values only.
3. **Subscribers.** Traits in the subscribing module's `application/`, wired in
   `server` (via 0002's module registration).
4. **Worker.** A `worker` bin, or `server --worker`, that polls outboxes and a
   Postgres job queue (`FOR UPDATE SKIP LOCKED`) with retries and dead-lettering.
   Scheduled jobs run on the same queue.
5. **Testing.** Drivers get `run_pending_events()` / `run_due_jobs()` verbs so tests
   stay synchronous and deterministic. Scheduling needs a controllable clock; build the injected one from spec §16 as part of this ticket.
6. **Reference example.** One cross-module reaction, for example deleting a user
   archives their notes (which also settles deletion semantics without foreign
   keys), and one side effect behind a port with a Fake.

## Acceptance criteria
- [ ] A service can save two aggregates in one transaction, and a failure rolls both back (covered by a test).
- [ ] A domain event saved with its aggregate is delivered to a subscriber in another module at least once (covered by a test).
- [ ] A failing handler is retried, then dead-lettered.
- [ ] The worker is deployable from the same image (documented in spec §6 next to `migrate`/`seed`).
- [ ] `CLAUDE.md` has a short "side effects and reactions" rule with a pointer to the reference.

## Open questions
- Hand-rolled outbox/queue, or a crate (`apalis`, `sqlxmq`, `pgmq` extension)? Weigh
  the pinned stack and MSRV.
- Should events be published types in `<module>-api` (a coupling surface) or opaque
  JSON with a schema?
- One outbox per module schema, or a shared `app.outbox` with a module column?
- How does a read module (D8) interact with this? Possibly via materialised
  projections once a provider is lifted out.

## Notes
This is the biggest change. Do it after 0001–0006 so it lands on a clean, enforced base.
