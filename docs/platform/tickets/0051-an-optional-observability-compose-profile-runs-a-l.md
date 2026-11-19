---
id: 0051
name: An optional observability compose profile runs a local OTLP backend for make dev
status: ready-for-agent
spec: docs/platform/specs/0003-traces-and-metrics-follow-every-request-and-event-through-opentelemetry.md
blocked_by: [docs/platform/tickets/0044-the-server-worker-and-migrate-export-traces-over-o.md]
blocks: [docs/platform/tickets/0052-adr-otlp-and-the-standard-variables-trace-context.md]
---

# An optional observability compose profile runs a local OTLP backend for make dev

Let a developer see traces and metrics locally without changing plain `make dev`.

## Acceptance criteria
- [ ] `docker-compose.yml` has an `observability` profile running an all-in-one OTLP backend (e.g. `grafana/otel-lgtm`), with its UI port and OTLP ports exposed.
- [ ] `docker compose up` without the profile doesn't start it.
- [ ] `make dev-observability`, listed in `make help`, starts the profile and runs `make dev` with `OTEL_EXPORTER_OTLP_ENDPOINT` pointing at it, so the server and worker export to it.
- [ ] `make dev` without it is unchanged and exports nothing.
- [ ] `.env.example` (or the Makefile help) says how to point `make dev` at the local backend and where its UI is.
- [ ] `make check-changed` passes.
