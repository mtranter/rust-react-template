#!/usr/bin/env bash
# Applies the migrations of the crates with queries that `make migrate` doesn't
# run, the module template's and every unregistered reference module's, to
# $DATABASE_URL, so `sqlx prepare` can check every crate's queries there.
# Applying a registered one again changes nothing.
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."

for core in crates/module-template/core crates/reference/*/core; do
  ./scripts/migrate-schema.sh "$core"
done
