# One error body for every failure

Amended by [ADR 0044](0044-traces-and-metrics-are-exported-over-otlp-and-a-trace-crosses-the-outbox-in-the-envelope.md): `ErrorBody` also carries `traceId`, the request's W3C trace id, whenever a trace is recorded; and `app_web::TraceRequests`, not poem's `Tracing`, spans each request inside `RequestIdMiddleware`.

Errors are part of the API contract. Every operation returns `app_web::ApiResult<T>`, so the Spec declares the same error responses (400, 401, 403, 404, 409, 412, 428, 500; an unsafe operation also declares the `422` an `Idempotency-Key` sent with another request gets, ADR 0032) on each, all with one `ErrorBody` schema: `code` (a closed `ErrorCode` enum, the thing clients branch on, never `message`), `message`, optional `fields: [{path, message}]` naming the inputs at fault as the client names them (`authorId`), and `requestId`. poem's and poem-openapi's own failures (bad JSON, a missing field, an unknown enum value, failed auth, an unknown route) go through `app_web::render_error` via `catch_all_error`, so they have the same shape: `malformed_request` for unparseable input, as against `validation_error` for a broken rule. The Generated Client types every error as `ErrorBody`, and a form puts each field error on the input its `path` names.

`RequestIdMiddleware` is outermost, around poem's `Tracing`, so access logs and every response, errors included, carry `x-request-id`, and the body echoes it.

## Considered options

- **RFC 9457 `problem+json`.** A flat body is simpler for the Generated Client and for forms.
- **Declaring only the statuses each operation can return.** More precise, but every operation would need its own error set kept in step with its code.

## Consequences

- The Spec over-declares: a client sees error statuses an operation never returns.
- Test drivers map a refusal from its `code`, not its status (ADR 0004), and `app_web::testing` checks every refusal has this shape and echoes its request id.
