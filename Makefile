# Orchestrator for the full stack. Real file targets do the work; phony targets
# are ergonomic aliases. `make` uses its dependency graph for cross-stack builds.
#
# NOTE: the frontend/ tree is not scaffolded yet. Backend targets work today;
# the FE targets (node_modules, generate, typecheck) activate once it exists.

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
	@mkdir -p frontend
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
.PHONY: help setup dev generate watch-api migrate prepare typecheck check build \
        db-up db-down db-wait fmt clippy deny test test-db-clean spec clean _require-cargo-bin

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

prepare: _require-cargo-bin migrate ## Refresh the checked-in .sqlx offline query cache (needs migrated DB)
	SQLX_OFFLINE=false DATABASE_URL=$(DATABASE_URL) $(CARGO_BIN) sqlx prepare --workspace -- --all-targets

generate: frontend/openapi.json ## Dump spec + run FE codegen (codegen no-ops until FE exists)
	@if [ -f frontend/openapi-ts.config.ts ]; then $(MAKE) frontend/src/api; else echo "frontend not scaffolded yet; wrote openapi.json only"; fi

spec: frontend/openapi.json ## Just (re)generate the OpenAPI snapshot

watch-api: _require-cargo-bin ## Watch backend sources and regenerate the client on change
	$(CARGO_BIN) cargo-watch -w crates -s 'make generate'

dev: _require-cargo-bin migrate generate ## Run backend (:8080) and Vite (:5173) in parallel
	@if [ -d frontend ]; then \
	  $(MAKE) -j2 _dev-backend _dev-frontend; \
	else \
	  echo "frontend not scaffolded; running backend only"; $(MAKE) _dev-backend; \
	fi

_dev-backend:
	$(CARGO_BIN) cargo-watch -x 'run --bin server'

_dev-frontend:
	cd frontend && pnpm run dev

fmt: ## rustfmt check
	$(CARGO) fmt --all --check

clippy: ## clippy with per-crate disallowed-types, warnings as errors
	$(CARGO) clippy --workspace --all-targets -- -D warnings

deny: _require-cargo-bin ## cargo-deny bans/licenses/advisories
	$(CARGO_BIN) cargo-deny check

test: db-wait ## Backend tests (#[sqlx::test], one fresh DB per test on docker-compose Postgres)
	SQLX_OFFLINE=true DATABASE_URL=$(TEST_DATABASE_URL) $(CARGO) test --workspace

test-db-clean: db-wait ## Drop leftover per-test databases (failed or renamed tests leave them behind)
	@dbs=$$(docker compose exec -T postgres psql -U app -d postgres -Atc \
	  "select datname from pg_database where datname like '\_sqlx\_test\_%'"); \
	for db in $$dbs; do docker compose exec -T postgres dropdb -U app --force "$$db"; done; \
	docker compose exec -T postgres psql -U app -d postgres -qc "drop schema if exists _sqlx_test cascade"; \
	echo "dropped $$(echo $$dbs | wc -w | tr -d ' ') test database(s)"

typecheck: ## Frontend tsc (no-op until FE exists)
	@if [ -d frontend ]; then cd frontend && pnpm run typecheck; else echo "frontend not scaffolded; skipping"; fi

check: fmt clippy deny test typecheck ## CI mirror: fmt + clippy + deny + tests + typecheck

build: ## Production build: (vite build) then release binary embedding the SPA
	@if [ -d frontend ]; then \
	  cd frontend && pnpm run build && cd .. && \
	  SQLX_OFFLINE=true $(CARGO) build --release --bin server --features embed-frontend; \
	else \
	  echo "frontend not scaffolded; building backend only"; \
	  SQLX_OFFLINE=true $(CARGO) build --release --bin server; \
	fi

clean: ## Remove build artifacts
	$(CARGO) clean
	rm -rf frontend/openapi.json frontend/src/api
