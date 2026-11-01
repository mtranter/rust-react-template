#!/usr/bin/env bash
# Spec D8: a module's SQL (its `src/db/` and `migrations/`) may name its own
# schema, its own published `<schema>_api` views, and nothing else — except the
# other modules' published views it lists in its Cargo.toml:
#
#   [package.metadata.module]
#   reads = ["users_api.user_v1"]
#
# A grep, not a SQL parser: it knows every module's schema name, so it flags any
# `<schema>.<object>` text in those files, including a Rust expression that
# happens to look like one (rename the variable).
set -euo pipefail
cd "$(dirname "$0")/.."

modules=(crates/modules/*/core crates/module-template)

schema_of() {
  local file="$1/src/db/migrations.rs"
  [ -f "$file" ] && sed -nE 's/.*pub const SCHEMA: &str = "([a-z0-9_]+)".*/\1/p' "$file" || true
}

reads_of() {
  sed -nE 's/^reads = \[(.*)\]/\1/p' "$1/Cargo.toml" | grep -oE '"[^"]+"' | tr -d '"' || true
}

published=()
for module in "${modules[@]}"; do
  schema=$(schema_of "$module")
  [ -n "$schema" ] && published+=("$schema" "${schema}_api")
done
known=$(IFS='|'; echo "${published[*]}")

failed=0
for module in "${modules[@]}"; do
  own=$(schema_of "$module")
  reads=$(reads_of "$module")

  for read in $reads; do
    case "$read" in
      "${own}_api".* | "$own".*) ;;
      *_api.*) [[ "${read%%.*}" =~ ^($known)$ ]] && continue ;;
    esac
    echo "$module/Cargo.toml: reads \"$read\": only another module's <schema>_api views may be read"
    failed=1
  done

  sources=()
  [ -d "$module/src/db" ] && sources+=("$module/src/db")
  [ -d "$module/migrations" ] && sources+=("$module/migrations")
  [ ${#sources[@]} -eq 0 ] && continue

  while IFS= read -r hit; do
    location=${hit%:*}
    reference=$(echo "${hit##*:}" | sed -E 's/^[^a-z]//')
    schema=${reference%%.*}
    if [ "$schema" = "$own" ] || [ "$schema" = "${own}_api" ]; then
      continue
    fi
    if echo "$reads" | grep -qxF "$reference"; then
      continue
    fi
    echo "$location: $reference is another module's; read only its published views, declared in $module/Cargo.toml"
    failed=1
  done < <(grep -rnoE "(^|[^a-zA-Z0-9_:])($known)\.[a-z0-9_]+" "${sources[@]}" || true)
done

if [ "$failed" -ne 0 ]; then
  echo "cross-module SQL check failed (spec D8)" >&2
  exit 1
fi
echo "cross-module SQL references ok"
