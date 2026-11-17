#!/usr/bin/env bash
# Every module core crate (`crates/modules/*/core`) must be registered everywhere
# the server and CI look for it — a missing entry compiles and passes the
# module's own tests, then fails only in a deployed environment. A reference
# module (`crates/reference/*/core`) is registered with the server everywhere or
# nowhere (`make init` unregisters them):
#
#   crates/server/src/modules.rs   its `Endpoints` in the tuple, its `module(..)` in the list
#   crates/server/Cargo.toml       a dependency, and `<module>/seed` in the `seed` feature
#                                  if it has a seed (a module that owns no data has none)
#   deny.toml                      both the `sqlx` and `poem` wrappers lists, registered or not
#
# It also checks that a module with a seed declares the number its seed ids are
# built from (`pub const MODULE: u32 = N;` in its `-api` crate's `src/seed.rs` if
# it has one, else its core's; ADR 0028), and that the numbers are unique.
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."

registry=crates/server/src/modules.rs
server=crates/server/Cargo.toml

failed=0
fail() {
  echo "$1"
  failed=1
}

# The registrations of the module at $1 the server has, one per line, each
# with what to say if it's missing.
registrations() {
  local module=$1 name ident endpoints
  name=$(sed -nE 's/^name = "([^"]+)"/\1/p' "$module/Cargo.toml" | head -1)
  ident=${name//-/_}
  endpoints=$(grep -oE '\b[A-Za-z]+Endpoints\b' "$module/src/lib.rs" | head -1 || true)
  if [ -z "$endpoints" ]; then
    echo "0 $module/src/lib.rs: exports no \`...Endpoints\`"
  else
    grep -qE "\b$ident::$endpoints\b" "$registry" && echo 1 || echo "0 $registry: \`$ident::$endpoints\` is missing from \`Endpoints\`"
  fi
  grep -qE "\b$ident::module\(" "$registry" && echo 1 || echo "0 $registry: \`$ident::module(..)\` is missing from \`modules()\`"
  sed -n '/^\[dependencies\]/,/^\[/p' "$server" | grep -qE "^$name(\.workspace = true| = )" &&
    echo 1 || echo "0 $server: \`$name\` is missing from [dependencies]"
  if [ -f "$module/src/seed.rs" ]; then
    grep -qE "^seed = \[.*\"$name/seed\"" "$server" && echo 1 ||
      echo "0 $server: \`\"$name/seed\"\` is missing from the \`seed\` feature"
  fi
}

numbers=()
for module in crates/modules/*/core crates/reference/*/core; do
  name=$(sed -nE 's/^name = "([^"]+)"/\1/p' "$module/Cargo.toml" | head -1)

  found=$(registrations "$module")
  if [[ $module == crates/modules/* ]] || grep -q '^1' <<<"$found"; then
    while IFS= read -r missing; do fail "${missing#0 }"; done < <(grep '^0' <<<"$found" || true)
  fi

  for crate in sqlx poem; do
    grep -qE "crate = \"$crate\", wrappers = \[.*\"$name\"" deny.toml ||
      fail "deny.toml: \`\"$name\"\` is missing from the \`$crate\` wrappers"
  done

  [ -f "$module/src/seed.rs" ] || continue
  seeds=("$module/src/seed.rs")
  [ -f "$module/../api/src/seed.rs" ] && seeds+=("$module/../api/src/seed.rs")
  number=$(sed -nE 's/^pub const MODULE: u32 = ([0-9]+);/\1/p' "${seeds[@]}")
  case $(printf '%s' "$number" | grep -c '^') in
    0) fail "$module: no \`pub const MODULE: u32\` in ${seeds[*]}" ;;
    1) numbers+=("$number $name") ;;
    *) fail "$module: \`pub const MODULE: u32\` is declared more than once in ${seeds[*]}" ;;
  esac
done

duplicates=$(printf '%s\n' "${numbers[@]}" | sort -n | awk '$1 == prev { print $1 } { prev = $1 }' | sort -u)
for number in $duplicates; do
  fail "module number $number is used by: $(printf '%s\n' "${numbers[@]}" | awk -v n="$number" '$1 == n { print $2 }' | xargs)"
done

if [ "$failed" -ne 0 ]; then
  echo "module registration check failed; \`make new-module\` registers a new module everywhere" >&2
  exit 1
fi
echo "module registration ok"
