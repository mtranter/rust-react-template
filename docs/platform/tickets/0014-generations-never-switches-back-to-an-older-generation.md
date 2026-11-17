---
id: 0014
name: "`Generations` never switches back to an older generation once a newer one is served"
status: done
priority: P2
size: S
blocked_by: []
blocks: [0017]
source: "PR 3 review (Copilot, \"Prevent cached generation updates from moving backwards\"), confirmed by reading 2026-11-15"
---

# `Generations` never switches back to an older generation once a newer one is served

## Description

`app_db::Generations::serving` documents "once a generation is served it stays served". Two concurrent requests can break that for a moment:

1. Request A reads the cache (`None`), finds v2 not caught up, and awaits the check of v1.
2. Meanwhile v2 catches up. Request B checks v2, caches index 0 (v2) and serves from v2.
3. Request A's check of v1 returns true. It overwrites the cache with index 1 (v1) and serves from v1.

The next request re-checks v2 and switches forward again, so it heals itself, and v1 is still a caught-up copy. But responses flip from the new shape back to the old one, which a client paging through results can see. The fallback path has the same flaw: it returns the `known` value read at the top of the call, which may be older than what another request has cached since.

## Evidence (as of 2026-11-15)

`crates/common/db/src/projection.rs`, `Generations::serving`:

- The cache is read once into `known`, the lock is released, and each `caught_up` check awaits the database.
- On success it does `*self.serving.lock() = Some(at)` unconditionally.
- When nothing newer has caught up it returns `self.newest_first[known]`, not the current cache.

`newest_first` is ordered newest first, so a *smaller* index is a *newer* generation.

## Proposed approach

Make the cache monotonic: under the mutex, only ever move to a smaller index, and always answer with what the cache holds after the update.

- On a successful check of index `at`: lock, set the cache to `min(cached, at)` (or `at` when empty), and return that generation.
- On the fallback path: lock and return the cached generation if there is one (it may be newer than `known`), else `unavailable`.

Put the update in one small private function over `Option<usize>` (for example `fn newest(serving: &mut Option<usize>, at: usize) -> usize`) so it can be tested without a database. Do not hold the mutex across an `.await`.

## Acceptance criteria

- [x] An inline unit test of the update function: caching an older index after a newer one leaves the newer one cached and returned. (`crates/AGENTS.md` allows inline tests for shared plumbing a driver can't provoke; this race is not reproducible deterministically through the database.)
- [x] The fallback path returns the cache's current value, not the value read at the start of the call.
- [x] The existing `Generations` tests in `crates/common/db/tests/delivery/projection.rs` pass unchanged.
- [x] `make check` passes.

## Notes

Ticket 0017 changes the same function (it logs when a generation is first served); do this one first.

## Resolution

`Generations::serving` now updates its cache through a private `newest(&mut Option<usize>, at) -> usize`, which keeps the smaller (newer) index and returns what it holds; `serving` answers with that generation. The fallback re-reads the cache under the lock instead of returning the value read at the start of the call. The mutex is never held across an `.await`. Inline test: `an_older_generation_caught_up_after_a_newer_one_is_served_leaves_the_newer_one_served`. The fallback change has no test of its own: the race needs two requests interleaved around a database await, which can't be provoked deterministically. ADR 0036 now says a served generation never switches back to an older one.
