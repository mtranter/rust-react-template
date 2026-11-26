# Orchestrator for the full stack. Real file targets do the work; phony targets
# are ergonomic aliases. `make` uses its dependency graph for cross-stack builds.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Load .env if present (local dev). CI sets these in the environment instead.
ifneq (,$(wildcard .env))
include .env
export
endif

DATABASE_URL ?= postgres://app:app@localhost:5432/app
export DATABASE_URL

# A recipe's git output on a terminal would open the pager, which with a
# `LESS` lacking `-F` waits for a keypress even on empty output.
export GIT_PAGER := cat

# `query!` compiles against the checked-in .sqlx cache, as in CI, so a fresh
# clone's empty database doesn't fail the build; `prepare` and `new-module`
# turn it off to refresh the cache.
SQLX_OFFLINE ?= true
export SQLX_OFFLINE

# The bus (ADR 0039): docker compose's Valkey, which `make dev` starts. Tests
# start their own (`app_bus::testing::Valkey`).
APP_REDIS_URL ?= redis://localhost:6379
export APP_REDIS_URL

# Backend tests (`#[app_test_db::test]`) use the same docker-compose server: each
# test binary migrates and seeds a `_test_template_*` database once, and each
# test gets a `_test_*` copy of it, created and dropped from the database named
# here — so point it at `postgres`, not the dev `app` database. SQLX_OFFLINE=true
# keeps `query!` compiling against the checked-in .sqlx cache rather than that
# (unmigrated) database.
TEST_DATABASE_URL ?= postgres://app:app@localhost:5432/postgres

# The packages test-backend runs; check-changed narrows it to what a change can break.
BACKEND_TESTS ?= --workspace

CARGO ?= cargo

# Repo-local Rust CLI tools (cargo-deny, cargo-watch, sqlx-cli) are declared in
# the root Cargo.toml [workspace.metadata.bin] and run via cargo-run-bin, which
# installs them into ./.bin instead of the global ~/.cargo/bin. `make setup`
# installs the single global bootstrap (cargo-run-bin) and then all the tools.
CARGO_BIN ?= cargo bin

# ---------------------------------------------------------------------------
# Real file targets (the dependency graph)
# ---------------------------------------------------------------------------

# Regenerate the OpenAPI snapshot whenever backend sources change. DB-free.
frontend/openapi.json: $(shell find crates -name '*.rs' 2>/dev/null) $(wildcard crates/server/Cargo.toml)
	SQLX_OFFLINE=true $(CARGO) run --quiet --bin dump-spec > frontend/openapi.json
	@echo "wrote frontend/openapi.json"

frontend/node_modules: frontend/pnpm-lock.yaml frontend/package.json
	cd frontend && pnpm install
	@touch frontend/node_modules

frontend/src/api: frontend/openapi.json frontend/openapi-ts.config.ts frontend/node_modules
	cd frontend && pnpm run generate
	@touch frontend/src/api

# ---------------------------------------------------------------------------
# Phony targets
# ---------------------------------------------------------------------------
.PHONY: help setup dev dev-observability generate watch-api migrate seed prepare new-module typecheck lint-frontend check build \
        check-backend check-frontend check-changed check-quick check-spec screens e2e \
        build-backend-dev build-frontend db-up redis-up db-down db-reset db-wait fmt clippy deny check-sql check-deps check-modules test test-backend \
        test-frontend test-db-clean spec clean _require-cargo-bin

help: ## Show this help
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

setup: ## Install repo-local Rust CLI tools (run once after cloning)
	@command -v cargo-bin >/dev/null 2>&1 || $(CARGO) install --locked cargo-run-bin
	$(CARGO_BIN) --install
	@echo "rust tools installed into ./.bin"

# Fail with a clear pointer instead of a cryptic error when the bootstrap is missing.
_require-cargo-bin:
	@command -v cargo-bin >/dev/null 2>&1 || { echo "cargo-run-bin missing — run 'make setup'"; exit 1; }

# Another checkout's compose Postgres may already hold :5432 (worktrees each get a
# compose project of their own); then this one's can't start, and that one serves.
db-up: ## Start local Postgres (docker compose), or use the one already on :5432
	@docker compose up -d --wait postgres || { \
	  timeout 2 bash -c '</dev/tcp/localhost/5432' 2>/dev/null \
	    && echo "using the Postgres already listening on :5432"; }

redis-up: ## Start local Valkey (docker compose), the bus `make dev` broadcasts over
	docker compose up -d --wait redis

db-wait: db-up ## Wait until Postgres is healthy (db-up waits)

db-down: ## Stop local Postgres
	docker compose down

db-reset: ## Delete local Postgres and its data volume, then migrate and seed afresh
	docker compose down -v postgres
	$(MAKE) seed

migrate: db-wait ## Apply all module migrations in dependency order
	$(CARGO) run --bin migrate

seed: migrate ## Load every module's standard seed data (skips what already exists)
	$(CARGO) run -p server --bin seed --features seed

prepare: _require-cargo-bin migrate ## Refresh the checked-in .sqlx offline query cache (needs migrated DB)
	./scripts/migrate-unregistered.sh
	SQLX_OFFLINE=false DATABASE_URL=$(DATABASE_URL) $(CARGO_BIN) sqlx prepare --workspace -- --all-targets

# --- template only: `make init` removes this block ---
init: ## Once, first, in a fresh clone of the template: make it an app. make init NAME=crm TITLE="Acme CRM"
	@test -n "$(NAME)" -a -n "$(TITLE)" || { echo 'usage: make init NAME=<name> TITLE="<display name>"'; exit 1; }
	./scripts/init.sh "$(NAME)" "$(TITLE)"

check-init: ## Run `make init` on a scratch copy of this tree, then check what it made (part of check)
	./scripts/check-init.sh
.PHONY: init check-init
# --- end template only ---

new-module: _require-cargo-bin db-wait ## Scaffold a write module in its Capability and register it: make new-module CAPABILITY=crm NAME=contacts [ENTITY=contact]
	@test -n "$(CAPABILITY)" && test -n "$(NAME)" || { echo "usage: make new-module CAPABILITY=<capability> NAME=<module> [ENTITY=<aggregate>]"; exit 1; }
	./scripts/new-module.sh $(CAPABILITY) $(NAME) $(ENTITY)

generate: frontend/src/api ## Dump spec + run FE codegen

spec: frontend/openapi.json ## Just (re)generate the OpenAPI snapshot

watch-api: _require-cargo-bin ## Watch backend sources and regenerate the client on change
	$(CARGO_BIN) cargo-watch -w crates -s 'make generate'

dev: _require-cargo-bin seed generate redis-up ## Run backend (:8080), the worker and Vite (:5173) in parallel
	$(MAKE) -j3 _dev-backend _dev-worker _dev-frontend

# Command-line variables reach the recipes' environment and override .env's.
dev-observability: ## make dev, exporting traces and metrics to a local Grafana (:3000) from compose's `observability` profile
	docker compose --profile observability up -d otel-lgtm
	@echo "traces and metrics: http://localhost:3000 (Grafana, Explore: Tempo and Prometheus)"
	$(MAKE) dev OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318

# Watch only the Rust sources: from the repo root cargo-watch first walks
# everything, `target/` (often a million files) included, before it builds.
WATCH_RUST := -w crates -w Cargo.toml -w Cargo.lock

_dev-backend:
	$(CARGO_BIN) cargo-watch $(WATCH_RUST) -x 'run --bin server'

_dev-worker:
	$(CARGO_BIN) cargo-watch $(WATCH_RUST) -x 'run --bin worker'

_dev-frontend:
	cd frontend && pnpm run dev

fmt: ## rustfmt check
	$(CARGO) fmt --all --check

check-quick: ## Inner loop: type-check every backend target (no clippy, build or tests), seconds after an edit
	SQLX_OFFLINE=true $(CARGO) check --workspace --all-targets --features server/seed

clippy: ## clippy with per-crate disallowed-types, warnings as errors
	$(CARGO) clippy --workspace --all-targets --features server/seed -- -D warnings

deny: _require-cargo-bin ## cargo-deny bans/licenses/advisories
	$(CARGO_BIN) cargo-deny --log-level error check

build-backend-dev: ## Debug build of every crate, including the seed bin (no DB needed)
	SQLX_OFFLINE=true $(CARGO) build --workspace --features server/seed

check-sql: ## Modules' SQL names only their own schema and the published views they declare (ADR 0010)
	./scripts/check-sql-schemas.sh

check-deps: ## No module core depends on another, -api crates stay pure, write modules declare no reads (ADR 0017)
	./scripts/check-module-deps.sh

check-spec: ## The checked-in frontend/openapi.json is the backend's current spec (else `make spec` and commit it)
	@SQLX_OFFLINE=true $(CARGO) run --quiet --bin dump-spec | cmp -s - frontend/openapi.json \
	  || { echo "frontend/openapi.json is stale: run 'make spec' and commit it"; exit 1; }

check-modules: ## Every module is registered with the server, cargo-deny and the seed feature; module numbers are unique
	./scripts/check-modules.sh

test: test-backend test-frontend ## Backend + frontend tests

# nextest runs every test in a process of its own, all in parallel; no doc-tests.
test-backend: _require-cargo-bin db-wait ## Backend tests (each a copy of its binary's migrated, seeded template DB on docker-compose Postgres)
	SQLX_OFFLINE=true DATABASE_URL=$(TEST_DATABASE_URL) $(CARGO_BIN) cargo-nextest run $(BACKEND_TESTS)

test-frontend: frontend/src/api ## Frontend Vitest (MSW-mocked network; no backend needed)
	cd frontend && pnpm run test

test-db-clean: db-wait ## Drop leftover per-test databases (failed or renamed tests leave them behind)
	@dbs=$$(docker compose exec -T postgres psql -U app -d postgres -Atc \
	  "select datname from pg_database where datname like '\_test\_%' or datname like '\_sqlx\_test\_%'"); \
	for db in $$dbs; do docker compose exec -T postgres dropdb -U app --force "$$db"; done; \
	docker compose exec -T postgres psql -U app -d postgres -qc "drop schema if exists _sqlx_test cascade"; \
	echo "dropped $$(echo $$dbs | wc -w | tr -d ' ') test database(s)"

typecheck: frontend/src/api ## Frontend tsc
	cd frontend && pnpm run typecheck

lint-frontend: frontend/node_modules ## Biome lint + format check, and feature import boundaries (as CI runs them)
	cd frontend && pnpm biome ci . && pnpm run lint:imports

screens: frontend/src/api ## Screenshot every UI scenario → frontend/screens/out/ (needs Playwright's Chromium; not in check)
	cd frontend && pnpm run screens

e2e: build-frontend db-wait _require-cargo-bin ## Sign in to the debug binary's embedded SPA through a fake OIDC provider, open /access, sign out (Playwright; part of check)
	./scripts/e2e.sh

build-frontend: frontend/src/api ## tsc + vite build → frontend/dist
	cd frontend && pnpm run build

# Every step CI runs, backend and frontend. A step added here is added to CI,
# and the reverse.
check: check-backend check-frontend e2e check-init ## CI mirror (run before calling work done)

check-backend: fmt clippy deny check-sql check-deps check-modules check-spec build-backend-dev test-backend ## The backend half of check

check-frontend: typecheck lint-frontend test-frontend build-frontend ## The frontend half of check

CHECK_BASE ?= main
check-changed: ## Only the halves of check this branch's changes (since CHECK_BASE, default main) can break
	./scripts/check-changed.sh $(CHECK_BASE)

build: build-frontend ## Production build: vite build, then the release binaries (the server embeds the SPA)
	SQLX_OFFLINE=true $(CARGO) build --release --bin server --bin worker --features embed-frontend

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/openapi.json frontend/src/api
