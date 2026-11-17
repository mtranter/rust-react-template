#!/usr/bin/env bash
# Applies one module's migrations to $DATABASE_URL as `make migrate` would: its
# own schema on the search path, so its `_sqlx_migrations` lands there (ADR 0021).
# For crates `make migrate` doesn't run: the module template, the reference
# modules once `make init` has unregistered them, and a just-scaffolded module
# whose queries `.sqlx` doesn't know yet, so `migrate` can't build.
#
#   scripts/migrate-schema.sh <crate dir> [<schema>]
#
# <schema> defaults to the crate's `SCHEMA` (in `src/db/migrations.rs`).
set -euo pipefail
cd "$(dirname "$0")/.."

dir=$1
schema=${2:-$(sed -nE 's/^pub const SCHEMA: &str = "([a-z0-9_]+)";/\1/p' "$dir/src/db/migrations.rs")}
[ -n "$schema" ] || { echo "migrate-schema: $dir names no SCHEMA: pass it" >&2; exit 1; }
DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}

docker compose exec -T postgres psql -q -v ON_ERROR_STOP=1 "$DATABASE_URL" \
  -c "CREATE SCHEMA IF NOT EXISTS \"$schema\""
separator='?'
[[ $DATABASE_URL == *\?* ]] && separator='&'
cargo bin sqlx migrate run --source "$dir/migrations" \
  --database-url "$DATABASE_URL${separator}options=-csearch_path%3D$schema"
