# Learnings: 0051 observability compose profile

- A compose service with `profiles: [observability]` is invisible to plain `docker compose up` / `config --services`; `docker compose --profile observability config --services` lists it. Use that to check a profile without starting anything.
- Variables given on `$(MAKE)`'s command line (`$(MAKE) dev OTEL_EXPORTER_OTLP_ENDPOINT=...`) reach every recipe's environment, sub-makes included, and override what the Makefile `include`s from `.env`. That is how `make dev-observability` points the server and worker at the backend without touching `make dev`.
- `grafana/otel-lgtm` takes OTLP on 4317 (gRPC) / 4318 (HTTP) within seconds, but Grafana on :3000 needs ~20–40s more before `/api/health` answers. The app's exporter is HTTP only (grpc isn't built in), so the endpoint is `http://localhost:4318`.
- Touching `docker-compose.yml` makes `make check-changed` run the whole backend lane with every test (see `scripts/check-changed.sh`), so expect a long run.
- To smoke-test a compose service without clashing with another checkout's containers, use a throwaway project: `docker compose -p <name> --profile observability up -d otel-lgtm`, then `down` with the same `-p`.
