---
id: 0003
name: Make errors part of the API contract, with field errors and a reference form
status: done
priority: P1
size: M
blocked_by: []
blocks: []
source: architecture review 2026-10-31, finding 4 (plus finding 9, request-id on errors)
---

# Make errors part of the API contract

## Description
The OpenAPI spec declares only `200` for every operation, so the generated client
has no error types. The type-safety net stops at the happy path. Errors come in two
shapes, validation errors carry no field path, and the reference form drops errors
silently. Every app's first feature is a form, so each team will invent its own
error parsing.

## Evidence (as of 2026-10-31)
- **Only `200` in the spec.** Every operation in `frontend/openapi.json` declares
  `responses: {200}` (verified).
- **Error body built outside the spec.** `crates/common/web/src/error.rs` builds
  `{"error":{code,message}}` by hand.
- **Framework errors come back in a different shape.** poem-openapi's own errors (bad
  JSON, missing field, unknown sort value) use poem's default body; there is no
  catch-all that normalises them.
- **No field detail on validation.** `AppError::Validation(String)` has no field path
  (`crates/common/error/src/lib.rs`).
- **The reference form drops a 400.**
  `frontend/src/features/notes/new-note-form.tsx` shows nothing on error (e.g. a
  title that's too long).
- **The request id is lost on errors.** `crates/common/web/src/request_id.rs` returns
  early through `?` when the handler errors, so failed responses lack `x-request-id`
  (verified). The error body doesn't carry the id either, and `Tracing` wraps
  `RequestIdMiddleware` from outside, so access logs miss the id.
- **Drivers can't tell a domain rule from a malformed body.** They map on status
  alone, so both come back as `Refusal::Invalid`.

## Proposed approach
1. **A shared error object.** Define `ErrorBody` in `app-web`: `code`, `message`,
   optional `fields: [{path, message}]` and `requestId`.
2. **Declare it on every operation.** Use a shared `ApiResponse<T>` enum or
   poem-openapi's error-response support, so the generated TS has typed error unions.
3. **Normalise framework errors.** Add a server-level `catch_all_error` that renders
   poem/poem-openapi errors as `ErrorBody`.
4. **Structured validation.** `AppError::Validation` gains structured field errors,
   and domain errors map onto field paths in each module's `application/error.rs`.
5. **Fix the request-id middleware.** Convert `Err` into a response before attaching
   the header, put the id in `ErrorBody`, and swap the `Tracing` and request-id
   middleware order.
6. **One frontend form pattern.** Pick a form library (TanStack Form or
   react-hook-form) with a helper that maps `fields` onto inputs, and a global
   `QueryClient` toast for non-field errors. Show it in `NewNoteForm`.
7. **Drivers assert on error codes.** They map `code`, not just status, so a test can
   tell a domain refusal from a malformed request.

## Acceptance criteria
- [x] Every operation in `openapi.json` declares its error responses using one `ErrorBody` schema.
- [x] A malformed body, an unknown enum value and a domain validation error all return the same shape.
- [x] Every error response carries `x-request-id`, and the same id appears in the body.
- [x] `NewNoteForm` shows a too-long-title error next to the Title field (covered by an MSW test).
- [x] At least one backend test asserts on a field error's path.

## Resolved questions
- **Envelope:** a flat `ErrorBody` (`code`, `message`, `fields?`, `requestId?`), not RFC 9457 and not the old `{error: {...}}` wrapper. `code` is a closed `ErrorCode` enum in the spec.
- **Statuses:** the same set (400/401/403/404/409/412/428/500) on every operation, from one `ApiError`: simpler, less precise.
- **Form library:** TanStack Form, matching the rest of the TanStack stack.
