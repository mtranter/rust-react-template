# Learnings: 0045 ErrorBody carries traceId

- `frontend/src/api/` is gitignored: `make generate` changes only `frontend/openapi.json` in git. Tickets asking to "commit `src/api/`" are satisfied by committing the snapshot.
- The current trace id comes from `tracing::Span::current().context()` (tracing-opentelemetry's `OpenTelemetrySpanExt`). With no OpenTelemetry layer installed the context is empty and its span context invalid, so "no trace" falls out with no extra config check. See `app_web::trace::current_trace_id`.
- `render_error` runs inside `TraceRequests`' instrumented future (catch_all_error sits inside it), so `Span::current()` there is the request span; a body built by a handler is too.
- `app_web::testing`'s `ErrorShape` uses `deny_unknown_fields` so any new `ErrorBody` field must be added there or every driver test fails.
- `app_web::testing::Traces::install()` gives a per-thread in-memory exporter; tests without it see no trace, which is how "tracing off" is tested.
- `make check-spec` prints a pre-existing `dead_code` warning (`RuleSet::unassign`); it is not a failure.
- Worktrees share the host's port 5432 and, if pointed at one Postgres, collide on test template DB names (`_test_template_<crate>`) when run concurrently. What worked: a compose override (`ports: !override ["5442:5432"]`) via `COMPOSE_FILE=docker-compose.yml:<override>`, plus `DATABASE_URL`/`TEST_DATABASE_URL` on port 5442, when running `make check-changed`.
