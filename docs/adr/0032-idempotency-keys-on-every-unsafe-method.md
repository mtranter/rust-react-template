# An Idempotency Key makes any unsafe request safe to retry

A client that loses a response can't tell whether its request ran. Retrying a POST may create twice; retrying a PATCH or DELETE that did run gets a spurious `412` (its `If-Match` is now stale, ADR 0016) or `404`. So any POST, PUT, PATCH or DELETE may carry an `Idempotency-Key` header, and a shared poem middleware in `app-web` runs it at most once per key:

- The first request with a key claims it and runs. A successful (2xx) answer is kept for 24 hours and replayed to a retry with the same key and the same request, which doesn't run again.
- The same key with a different request (method, path and query, `If-Match` and body, fingerprinted: a retry expecting another version is another request) is refused: `idempotency_key_reused`, `422`.
- A retry while the first is still running is refused as in flight (`409`), to retry later. A claim left unanswered for a minute (its client gone, or the server crashed mid-request) counts as abandoned, and a retry may take it and run.
- A failure isn't kept: the claim is released, so a retry runs again.

Keys are scoped to the caller: the bearer token's subject when signed in, else one anonymous scope. Records live in Postgres, in a shared schema owned by `app-db`, as `messaging` is (ADR 0011): they belong to no Module, and each claim is one conditional statement.

## Considered options

- **Only POST** (as Flagrante, the project this was taken from, does). Leaves PATCH and DELETE retries to fail spuriously after a lost response.
- **Scoping keys by Tenant.** The template has no Tenant; the token's subject is the nearest owner.
- **Per-Module idempotency in each use case.** Every Module would reimplement claiming, fingerprinting and replay.
- **Redis or another store.** A second dependency, when Postgres is already there.

## Consequences

- A request still running after a minute may run twice: a minute is well past any answer the API gives.
- Without a key, a request behaves as before. Keys are optional, and clients generate one per user intent.
- Unsafe operations declare the header and the `422` in the Spec through `app_web::Idempotent`, which wraps the server's `Endpoints`, so an operation added later is documented as it's served.
- The middleware sits inside the request id, so its refusals carry one, and outside the error rendering, so a replay is the answer the client first got.
- There are no scheduled jobs (ADR 0015), so each claim deletes the records past their 24 hours, over an index on when they were claimed.
