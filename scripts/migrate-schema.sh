#!/usr/bin/env bash
# Applies one module's migrations to $DATABASE_URL as `make migrate` would: its
# own schema on the search path, so its `_sqlx_migrations` lands there (spec §6).
# For crates `make migrate` doesn't run: the module template, and a just-scaffolded
# module whose queries `.sqlx` doesn't know yet, so `migrate` can't build.
#
#   scripts/migrate-schema.sh <crate dir> <schema>
set -euo pipefail
cd "$(dirname "$0")/.."

dir=$1
schema=$2
DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}

docker compose exec -T postgres psql -q -v ON_ERROR_STOP=1 "$DATABASE_URL" \
  -c "CREATE SCHEMA IF NOT EXISTS \"$schema\""
separator='?'
[[ $DATABASE_URL == *\?* ]] && separator='&'
cargo bin sqlx migrate run --source "$dir/migrations" \
  --database-url "$DATABASE_URL${separator}options=-csearch_path%3D$schema"
