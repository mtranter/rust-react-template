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

# Backend tests use `#[sqlx::test]` against the same docker-compose server: it
# creates a fresh `_sqlx_test_*` database per test and keeps bookkeeping (a
# `_sqlx_test` schema) in the database named here — so point it at `postgres`,
# not the dev `app` database. SQLX_OFFLINE=true keeps `query!` compiling against
# the checked-in .sqlx cache rather than that (unmigrated) database.
TEST_DATABASE_URL ?= postgres://app:app@localhost:5432/postgres

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
.PHONY: help setup dev generate watch-api migrate seed prepare new-module typecheck lint-frontend check build \
        build-backend-dev build-frontend db-up db-down db-wait fmt clippy deny check-sql check-deps check-modules test test-backend \
        test-frontend test-db-clean spec clean _require-cargo-bin

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

setup: ## Install repo-local Rust CLI tools (run once after cloning)
	@command -v cargo-bin >/dev/null 2>&1 || $(CARGO) install --locked cargo-run-bin
	$(CARGO_BIN) --install
	@echo "rust tools installed into ./.bin"
	@command -v lefthook >/dev/null 2>&1 || echo "note: lefthook not found — 'brew install lefthook' (or 'pnpm add -g lefthook'), then 'lefthook install'"

# Fail with a clear pointer instead of a cryptic error when the bootstrap is missing.
_require-cargo-bin:
	@command -v cargo-bin >/dev/null 2>&1 || { echo "cargo-run-bin missing — run 'make setup'"; exit 1; }

db-up: ## Start local Postgres (docker compose)
	docker compose up -d postgres

db-wait: db-up ## Wait until Postgres is healthy
	@echo "waiting for postgres..."; \
	until docker compose exec -T postgres pg_isready -U app -d app >/dev/null 2>&1; do sleep 1; done; \
	echo "postgres ready"

db-down: ## Stop local Postgres
	docker compose down

migrate: db-wait ## Apply all module migrations in dependency order
	$(CARGO) run --bin migrate

seed: migrate ## Load every module's standard seed data (skips what already exists)
	$(CARGO) run -p server --bin seed --features seed

prepare: _require-cargo-bin migrate ## Refresh the checked-in .sqlx offline query cache (needs migrated DB)
	./scripts/migrate-schema.sh crates/module-template/core widgets
	SQLX_OFFLINE=false DATABASE_URL=$(DATABASE_URL) $(CARGO_BIN) sqlx prepare --workspace -- --all-targets

new-module: _require-cargo-bin db-wait ## Scaffold a write module and register it: make new-module NAME=widgets [ENTITY=widget]
	@test -n "$(NAME)" || { echo "usage: make new-module NAME=<module> [ENTITY=<aggregate>]"; exit 1; }
	./scripts/new-module.sh $(NAME) $(ENTITY)

generate: frontend/src/api ## Dump spec + run FE codegen

spec: frontend/openapi.json ## Just (re)generate the OpenAPI snapshot

watch-api: _require-cargo-bin ## Watch backend sources and regenerate the client on change
	$(CARGO_BIN) cargo-watch -w crates -s 'make generate'

dev: _require-cargo-bin seed generate ## Run backend (:8080), the worker and Vite (:5173) in parallel
	$(MAKE) -j3 _dev-backend _dev-worker _dev-frontend

_dev-backend:
	$(CARGO_BIN) cargo-watch -x 'run --bin server'

_dev-worker:
	$(CARGO_BIN) cargo-watch -x 'run --bin worker'

_dev-frontend:
	cd frontend && pnpm run dev

fmt: ## rustfmt check
	$(CARGO) fmt --all --check

clippy: ## clippy with per-crate disallowed-types, warnings as errors
	$(CARGO) clippy --workspace --all-targets --features server/seed -- -D warnings

deny: _require-cargo-bin ## cargo-deny bans/licenses/advisories
	$(CARGO_BIN) cargo-deny check

build-backend-dev: ## Debug build of every crate, including the seed bin (no DB needed)
	SQLX_OFFLINE=true $(CARGO) build --workspace --features server/seed

check-sql: ## Modules' SQL names only their own schema and the published views they declare (spec D8)
	./scripts/check-sql-schemas.sh

check-deps: ## No module core depends on another, -api crates stay pure, write modules declare no reads (spec §4)
	./scripts/check-module-deps.sh

check-modules: ## Every module is registered with the server, cargo-deny and the seed feature; module numbers are unique
	./scripts/check-modules.sh

test: test-backend test-frontend ## Backend + frontend tests

test-backend: db-wait ## Backend tests (#[sqlx::test], one fresh DB per test on docker-compose Postgres)
	SQLX_OFFLINE=true DATABASE_URL=$(TEST_DATABASE_URL) $(CARGO) test --workspace

test-frontend: frontend/src/api ## Frontend Vitest (MSW-mocked network; no backend needed)
	cd frontend && pnpm run test

test-db-clean: db-wait ## Drop leftover per-test databases (failed or renamed tests leave them behind)
	@dbs=$$(docker compose exec -T postgres psql -U app -d postgres -Atc \
	  "select datname from pg_database where datname like '\_sqlx\_test\_%'"); \
	for db in $$dbs; do docker compose exec -T postgres dropdb -U app --force "$$db"; done; \
	docker compose exec -T postgres psql -U app -d postgres -qc "drop schema if exists _sqlx_test cascade"; \
	echo "dropped $$(echo $$dbs | wc -w | tr -d ' ') test database(s)"

typecheck: frontend/src/api ## Frontend tsc
	cd frontend && pnpm run typecheck

lint-frontend: frontend/node_modules ## Biome lint + format check, and feature import boundaries (as CI runs them)
	cd frontend && pnpm biome ci . && pnpm run lint:imports

build-frontend: frontend/src/api ## tsc + vite build → frontend/dist
	cd frontend && pnpm run build

# Every step CI runs, backend and frontend.
check: fmt clippy deny check-sql check-deps check-modules build-backend-dev test typecheck lint-frontend build-frontend ## CI mirror (run before calling work done)

build: build-frontend ## Production build: vite build, then the release binaries (the server embeds the SPA)
	SQLX_OFFLINE=true $(CARGO) build --release --bin server --bin worker --features embed-frontend

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/openapi.json frontend/src/api
