# Every write is guarded by the root's version, sent as `If-Match`

Load → act → save has a lost-update window, so every write Module versions its Aggregates and there are no blind overwrites. The version is persistence state, not domain state: `app-persistence` holds it in `Versioned<T> { entity, version }` and `Expected::{New, Version(v)}`, and the Aggregate never sees it.

- **DB.** The root's table has `version bigint`. `save` inserts at `Version::FIRST` for `Expected::New`, and for `Expected::Version(v)` runs `UPDATE … SET …, version = version + 1 WHERE id = $1 AND version = v RETURNING version`, mapping no row to `PreconditionFailed`. The check is atomic in that statement. Never an upsert.
- **Application.** An edit takes the caller's expected version, loads the Aggregate first (so a missing one is not found rather than stale), acts, and saves with `Expected::Version(expected)`.
- **Web.** A single-resource response is `WithETag<XResponse>` (a strong, quoted `ETag`), and the body mirrors the version as `version: string` so the Generated Client can read it. An edit reads the expected version only from `If-Match`, never from the body: absent is `428 Precondition Required`, stale `412 Precondition Failed`.

## Considered options

- **The version in the request body.** Bypasses HTTP's own conditional-request semantics, which caches and clients already understand.
- **The version on the Aggregate.** Domain code would carry persistence state it never decides.

## Consequences

- The root's version also orders its Events (ADR 0013), and is the ETag of the root and every Child Entity (ADR 0030).
- A retried edit whose first answer was lost sees its own write as stale; an Idempotency Key avoids that (ADR 0032).
