---
id: 0017
name: Serving a projection that has dead-lettered deliveries is logged as an error
status: done
priority: P2
size: S
blocked_by: [0014]
blocks: []
source: evaluation of the pub-sub mechanism 2026-11-15; decision by Mark the same day (readiness must not refuse, but log loudly)
---

# Serving a projection that has dead-lettered deliveries is logged as an error

## Description

`app_db::caught_up` treats a dead-lettered job as done: a copy counts as caught up once every job of its backfill is "handled (or dead)". So if some backfill deliveries fail all their attempts, `Generations` starts serving a copy that is missing those keys (or holds them stale), with no sign on the serving side. The same is true of a live event dead-lettered later.

**Decision (2026-11-15): readiness does not refuse.** One poisoned event must not keep a whole read model answering 503, and the copy is still mostly right. Instead the condition is logged loudly, at **error** level. Error is the right level: data is known to be missing from what is being served, it will not fix itself, and an operator has to act (fix the cause, then rerun the dead letters). It also matches the level the worker already uses for the dead-lettering itself.

What exists today and what is missing:

- The worker already logs `job dead-lettered` at error for each one, when it happens (`Worker::fail`). Keep that.
- Nothing is logged where the copy is *served*. A server process that starts after the dead-lettering never mentions it, and the worker's line doesn't say a served read model is affected.

## Evidence (as of 2026-11-15)

- `crates/common/db/src/projection.rs` `caught_up`: the job condition includes `j.dead_at IS NULL`, so dead jobs are passed over. Its doc comment says "(or dead)".
- `crates/common/db/src/projection.rs` `Generations::serving`: logs nothing.
- `crates/common/db/src/worker.rs` `fail`: `tracing::error!(queue, attempts, error, "job dead-lettered")`.

## Proposed approach

Log when a `Generations` first serves a generation in a process, which is the moment it decides the copy is good enough.

1. Add a query in `crates/common/db/src/projection.rs` that counts dead letters per queue for a set of queues: `SELECT queue, count(*) FROM messaging.job WHERE queue = ANY($1) AND dead_at IS NOT NULL GROUP BY queue`. Expose it as a small public function (for example `app_db::dead_letters_in(pool, queues) -> AppResult<Vec<(String, i64)>>`), so it can be tested and so a module can use it for a health check later.
2. In `Generations::serving`, at the point a generation becomes the cached one (after ticket 0014 this is the single place the cache moves to a newer index), call it for that generation's queues. For each queue with dead letters, log:

   ```rust
   tracing::error!(
       queue,
       dead_letters,
       "serving a projection with dead-lettered deliveries: its copy is missing or stale for their keys until they are rerun"
   );
   ```

3. It runs once per generation per process, not per request: only when the cache changes. A failure of the counting query must not fail the request: log it at warn and serve.
4. Leave `caught_up`'s result and its "(or dead)" rule unchanged. Say in its doc comment, and in ADR 0036's paragraph on readiness, that dead letters don't hold readiness back and are reported at error when the copy is first served, and record the decision above in ADR 0036's alternatives ("Refusing to serve while any delivery is dead-lettered", rejected: one poisoned event would take the whole read model down).

Because every server process logs it at start-up for as long as the dead letters remain, the error repeats on each deploy until someone reruns or removes them. That is intended.

## Acceptance criteria

- [ ] A test in `crates/common/db/tests/delivery/projection.rs`: a projection whose backfill has one delivery dead-lettered (use `messaging.fails(key, version, MAX_ATTEMPTS)` and `messaging.later(..)` between runs, as the existing dead-letter tests do) is caught up and served by `Generations`, and the new function reports one dead letter for its queue.
- [ ] The new function reports nothing for a queue with no dead letters, and counts only the queues asked for.
- [ ] `Generations::serving` logs at error, once, when it first serves a generation whose queues have dead letters; it does not log per request. (The repo has no log-capture helper; assert the logging by test only if one is added cheaply, otherwise the function's test and review of the call site suffice.)
- [ ] A failing count does not turn a successful `serving()` into an error.
- [ ] ADR 0036 and the `caught_up` doc comment record the decision.
- [ ] `.sqlx` regenerated; `make check` passes.

## Notes

- An admin surface for dead letters is listed under "Not built yet" in `crates/AGENTS.md`; this ticket doesn't add one.
- Not in scope: a periodic re-check after the generation is latched. A dead letter that appears later is already logged at error by the worker when it happens.

## Resolution

Done as proposed. `app_db::dead_letters_in(pool, queues)` counts dead letters per queue for the queues asked. `Generations::serving` calls it only when its cache moves to a newer generation (once per generation per process, never per request) and logs each queue's count at error; a failing count is logged at warn and the generation is still served. `caught_up` is unchanged; its doc comment and ADR 0036 (readiness paragraph, and the rejected "refusing to serve while any delivery is dead-lettered") record the decision.

The logging is tested: the delivery tests gained a small capture of error events (`LoggedErrors`, with `tracing-subscriber` as an `app-db` dev-dependency). The failing-count path has no test: the count reads only what `caught_up` reads just before it, so a test can't make one fail and not the other; it rests on review of `report_dead_letters`.
