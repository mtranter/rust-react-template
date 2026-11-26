#!/usr/bin/env bash
# `make e2e`: builds the server (debug, with `embed-frontend`, over the
# `frontend/dist` `make build-frontend` made) and `fake-oidc`, migrates and seeds
# a database of its own on the docker compose Postgres, then runs
# `frontend/e2e/sign-in.ts`, which starts both and signs in through the SPA,
# and connects the reference integration to its fake (`E2E_FAKE`).
# The database is dropped afterwards; one a killed run leaves is `_test_e2e_*`,
# which `make test-db-clean` drops.
set -euo pipefail
cd "$(dirname "$0")/.."

target=$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)
bin=$target/debug
# The integration the e2e connects: a fake of the other system, its Connection's key and the
# secret it takes. `make init` drops it with the reference code.
fake=(fake-noticeboard noticeboard token)
SQLX_OFFLINE=true ${CARGO:-cargo} build --bin server --bin migrate --bin seed --bin fake-oidc \
  ${fake:+--bin "${fake[0]}"} --features server/embed-frontend,server/seed

# Its own name, so runs from several checkouts on one Postgres don't collide.
database_url=${E2E_SERVER_URL:-postgres://app:app@localhost:5432}/_test_e2e_$$
sqlx() { ${CARGO_BIN:-cargo bin} sqlx database "$@" --database-url "$database_url"; }
trap 'sqlx drop -y -f >/dev/null' EXIT
sqlx create
DATABASE_URL=$database_url "$bin/migrate" >/dev/null
DATABASE_URL=$database_url "$bin/seed" >/dev/null

cd frontend
E2E_FAKE=${fake:+"$bin/${fake[0]} ${fake[1]} ${fake[2]}"} \
  E2E_BIN=$bin E2E_DATABASE_URL=$database_url node e2e/sign-in.ts
