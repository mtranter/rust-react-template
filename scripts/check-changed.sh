#!/usr/bin/env bash
# Runs only the `make check` lanes this branch's changes can break, against
# the merge base with $1 (default main), including uncommitted changes.
# A path it doesn't know runs both lanes.
#
# The backend lane tests only the crates a change can break: each changed
# crate and the workspace crates depending on it directly. Modules' tests stub
# what's across their boundaries, and the crates composing modules depend on
# them (`server` on every registered module, `reference-journeys` on the
# reference modules), so a module's change runs its own tests and the composed
# app's. The platform's modules (`crates/platform/`) are modules like the app's. A change to what every crate shares (common, the lockfile, the .sqlx
# cache) tests the whole workspace.
#
# The end-to-end check (`make e2e`) runs when what it signs in through can
# break: the SPA, the server, `app-web` (the token's verification) or the fake
# provider itself.
set -euo pipefail

base=${1:-main}
since=$(git merge-base "$base" HEAD)

backend=false
frontend=false
all_tests=false
e2e=false
crates=()

while IFS= read -r path; do
  [ -n "$path" ] || continue
  case "$path" in
    docs/* | *.md) ;;
    # The client is generated from it, so a changed spec can break the frontend;
    # check-backend's check-spec proves it matches the backend that made it.
    frontend/openapi.json) backend=true frontend=true e2e=true ;;
    frontend/*) frontend=true e2e=true ;;
    crates/reference/journeys/*) backend=true crates+=(crates/reference/journeys) ;;
    crates/modules/*/*/* | crates/platform/*/*/* | crates/reference/*/*/*) backend=true crates+=("$(cut -d/ -f1-4 <<<"$path")") ;;
    crates/module-template/*/*) backend=true crates+=("$(cut -d/ -f1-3 <<<"$path")") ;;
    crates/server/*) backend=true e2e=true crates+=(crates/server) ;;
    crates/common/web/* | crates/common/fake-oidc/*) backend=true all_tests=true e2e=true ;;
    scripts/e2e.sh) e2e=true ;;
    crates/* | .sqlx/* | Cargo.toml | Cargo.lock | deny.toml | rust-toolchain.toml | .cargo/* | \
      docker-compose.yml | scripts/check-*.sh | scripts/migrate-*.sh | scripts/new-module.sh | \
      scripts/rustc-wrapper.sh)
      backend=true all_tests=true ;;
    *) backend=true frontend=true all_tests=true e2e=true ;;
  esac
done < <(
  git diff --name-only "$since"
  git ls-files --others --exclude-standard
)

package_of() { sed -n 's/^name *= *"\(.*\)"/\1/p' "$1/Cargo.toml" | head -1; }

packages=()
if ! $all_tests && [ ${#crates[@]} -gt 0 ]; then
  while IFS= read -r crate; do
    # A deleted crate leaves nothing to test, but whatever used it can break.
    if [ ! -f "$crate/Cargo.toml" ]; then all_tests=true; break; fi
    package=$(package_of "$crate")
    packages+=("$package")
    for manifest in crates/modules/*/*/Cargo.toml crates/platform/*/*/Cargo.toml crates/reference/*/*/Cargo.toml crates/reference/journeys/Cargo.toml \
      crates/module-template/*/Cargo.toml crates/server/Cargo.toml; do
      [ -f "$manifest" ] || continue
      grep -Eq "^$package(\.workspace| *=)" "$manifest" && packages+=("$(package_of "$(dirname "$manifest")")")
    done
  done < <(printf '%s\n' "${crates[@]}" | sort -u)
fi

lanes=()
if $backend; then
  if $all_tests; then
    :
  elif [ ${#packages[@]} -gt 0 ]; then
    lanes+=("BACKEND_TESTS=$(printf -- '-p %s\n' "${packages[@]}" | sort -u | tr '\n' ' ')")
  else
    lanes+=(-o test-backend)
  fi
  lanes+=(check-backend)
fi
if $frontend; then
  # Frontend-only: the checked-in spec is current, so don't rebuild the backend to regenerate it.
  $backend || lanes+=(-o frontend/openapi.json)
  lanes+=(check-frontend)
fi

$e2e && lanes+=(e2e)

if [ ${#lanes[@]} -eq 0 ]; then
  echo "check-changed: nothing since $base that make check covers"
  exit 0
fi
echo "check-changed: make ${lanes[*]}"
exec "${MAKE:-make}" "${lanes[@]}"
