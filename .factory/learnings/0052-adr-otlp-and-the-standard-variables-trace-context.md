# Learnings: 0052 ADR on OTLP, trace context and the metric set

- The decision is ADR 0044 (`docs/platform/adr/0044-…`). ADR numbers are separate from ticket numbers: ticket 0044 (the OTLP exporter) is not ADR 0044.
- Most of the doc work had already landed with tickets 0044–0051: `crates/AGENTS.md`'s "Traces and metrics" section, the `add-telemetry` skill and `.env.example` were current, and "Not built yet" no longer listed an exporter. Check what's there before rewriting.
- To get the exact metric names for a doc, grep the string literals: `grep -rhoE '"(http|mcp|idempotency|db|messaging)\.[a-z_.]+"' crates/common/web/src crates/common/db/src`.
- An ADR amended by a later one gets an `Amended by [ADR n](file): …` line under its title (see 0009, 0010, 0040), not an edit to its body.
- ADR 0022 still said poem's `Tracing` was inside `RequestIdMiddleware`; it's `app_web::TraceRequests` now. Its amendment note records that.
- The sampler isn't set in code: the SDK reads `OTEL_TRACES_SAMPLER`, and its default is `parentbased_always_on`.
