#!/usr/bin/env bash
# Runs only the `make check` lanes this branch's changes can break, against
# the merge base with $1 (default main), including uncommitted changes.
# A path it doesn't know runs both lanes.
set -euo pipefail

base=${1:-main}
since=$(git merge-base "$base" HEAD)

backend=false
frontend=false

while IFS= read -r path; do
  [ -n "$path" ] || continue
  case "$path" in
    docs/* | *.md) ;;
    # The client is generated from it, so a changed spec can break the frontend;
    # check-backend's check-spec proves it matches the backend that made it.
    frontend/openapi.json) backend=true frontend=true ;;
    frontend/*) frontend=true ;;
    crates/* | .sqlx/* | Cargo.toml | Cargo.lock | deny.toml | rust-toolchain.toml | \
      docker-compose.yml | scripts/check-*.sh | scripts/migrate-schema.sh | scripts/new-module.sh | \
      scripts/new-projection.sh)
      backend=true ;;
    *) backend=true frontend=true ;;
  esac
done < <(
  git diff --name-only "$since"
  git ls-files --others --exclude-standard
)

lanes=()
$backend && lanes+=(check-backend)
if $frontend; then
  # Frontend-only: the checked-in spec is current, so don't rebuild the backend to regenerate it.
  $backend || lanes+=(-o frontend/openapi.json)
  lanes+=(check-frontend)
fi

if [ ${#lanes[@]} -eq 0 ]; then
  echo "check-changed: nothing since $base that make check covers"
  exit 0
fi
echo "check-changed: make ${lanes[*]}"
exec "${MAKE:-make}" "${lanes[@]}"
