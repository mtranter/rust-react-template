# Learnings: 0047 MCP tool call spans

- The app's `TraceRequests` sits outside the whole router, so it sees only `POST /mcp`; the in-process call a tool makes bypasses it. `mcp::endpoint` therefore wraps the API it calls in its own `TraceRequests::of::<S::Api>(S::BASE)`, which makes the REST span a child of the current (tool) span: the in-process request carries no `traceparent`.
- The tool span is opened in `OpenApiTools::call` with `otel.name = name` (rename only works before the span starts), and `is_error` / `otel.status_code` declared `Empty` and recorded after the call.
- `app_web::testing::Traces` is per-thread; `#[tokio::test]`'s current-thread runtime keeps the MCP session's work on the test thread, so the spans are captured.
- A first session ran out without returning structured output; its uncommitted work was complete and passing, and was picked up as is.
- `make check-changed` here: 5432 was held by another checkout's Postgres; ran it with `COMPOSE_FILE` pointing at a scratch compose file (port 5447), `COMPOSE_PROJECT_NAME`, and `TEST_DATABASE_URL`/`DATABASE_URL` on 5447.
