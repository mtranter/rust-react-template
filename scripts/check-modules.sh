#!/usr/bin/env bash
# Every module core crate (`crates/capabilities/*/*/core`, and the platform's in
# `crates/platform/*/core`) must be registered everywhere the server and CI look
# for it — a missing entry compiles and passes the module's own tests, then
# fails only in a deployed environment. A reference
# module (`crates/reference/*/core`) is registered with the server everywhere or
# nowhere (`make init` unregisters them):
#
#   crates/server/src/modules.rs   its `Endpoints` in the tuple, its `module(..)` in the list
#   crates/server/Cargo.toml       a dependency, and `<module>/seed` in the `seed` feature
#                                  if it has a seed (a module that owns no data has none)
#   deny.toml                      both the `sqlx` and `poem` wrappers lists, registered or not
#
# A module with operations must have policies (ADR 0040): its `di.rs` registers
# them with `Module::with_policies(..)`, with its resources and default
# grants, and its web layer's `OPERATIONS` with `.performing(..)`.
#
# Every Module sits in a Capability's folder (ADR 0047), beside its
# `capability.yaml`: `crates/capabilities/<capability>/`, whose id is the
# folder's name; each of the platform's `crates/platform/<module>/`, whose id is
# the Module's; and `crates/reference/`. Its `di.rs` names that file in its
# policies. Each Capability folder holds a Module, and each live Capability (the
# reference's only while its Modules are registered) has an entry in the SPA's
# registry, frontend/src/shell/capabilities.ts, as a literal `id: "<id>"`, but
# `assistant` and `connections`, which have no app; each entry there names a live Capability. Each
# of its Areas' `needs: { resource: "<r>", action: "<a>" }`, a literal on one
# line, names an action a registered Module's policies declare: compared with
# what `dump-spec --actions` lists, as the generated client's types hold the
# catalog's shape but not its resources' names.
#
# It also checks that a module with a seed declares the number its seed ids are
# built from (`pub const MODULE: u32 = N;` in its `-api` crate's `src/seed.rs` if
# it has one, else its core's; ADR 0028), and that the numbers are unique.
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."

registry=crates/server/src/modules.rs
server=crates/server/Cargo.toml
spa=frontend/src/shell/capabilities.ts
# Capabilities with no app in the SPA. `connections`' screen is to come (ticket 0081).
appless=(assistant connections)

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

id_of() { sed -nE 's/^id: *"?([^"]*)"? *$/\1/p' "$1" | head -1; }

# The folder of the Capability the Module at $1 sits in, and the id the folder
# gives it (none for the reference's).
capability_of() {
  case $1 in
    crates/capabilities/*) echo "$(dirname "$(dirname "$1")") $(basename "$(dirname "$(dirname "$1")")")" ;;
    crates/platform/*) echo "$(dirname "$1") $(basename "$(dirname "$1")")" ;;
    crates/reference/*) echo "crates/reference " ;;
  esac
}

for folder in crates/capabilities/*/; do
  folder=${folder%/}
  cores=("$folder"/*/core)
  ((${#cores[@]})) || fail "$folder: a Capability with no Module: give it one (\`make new-module CAPABILITY=$(basename "$folder") NAME=..\`) or remove it"
done

numbers=()
live=()
for module in crates/capabilities/*/*/core crates/platform/*/core crates/reference/*/core; do
  name=$(sed -nE 's/^name = "([^"]+)"/\1/p' "$module/Cargo.toml" | head -1)

  found=$(registrations "$module")
  registered=false
  if [[ $module == crates/capabilities/* || $module == crates/platform/* ]] || grep -q '^1' <<<"$found"; then
    registered=true
    while IFS= read -r missing; do fail "${missing#0 }"; done < <(grep '^0' <<<"$found" || true)
  fi

  read -r folder expected <<<"$(capability_of "$module")"
  if [ ! -f "$folder/capability.yaml" ]; then
    fail "$module: no capability.yaml above it, in $folder: every Module sits in a Capability's folder (ADR 0047)"
  else
    id=$(id_of "$folder/capability.yaml")
    [ -z "$expected" ] || [ "$id" = "$expected" ] ||
      fail "$folder/capability.yaml: its id is \`$id\`, but its folder makes it \`$expected\`"
    path=../../capability.yaml
    [[ $module == crates/platform/* ]] && path=../capability.yaml
    if grep -qE '\.with_policies\(' "$module/src/di.rs" 2>/dev/null; then
      grep -qF "policy_file!(\"$path\")" "$module/src/di.rs" ||
        fail "$module/src/di.rs: its policies don't name its Capability: \`.capability(policy_file!(\"$path\"))\`"
    fi
    if $registered; then live+=("$id"); fi
  fi

  if grep -qE 'operation_id = ' "$module"/src/web/*.rs 2>/dev/null; then
    grep -qE '\.with_policies\(' "$module/src/di.rs" ||
      fail "$module/src/di.rs: it has operations but no policies: register them with \`Module::with_policies(..)\`"
    grep -qE '\.performing\(' "$module/src/di.rs" ||
      fail "$module/src/di.rs: it has operations but doesn't register its \`OPERATIONS\` with \`.performing(..)\`"
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

ids=$(sed -nE 's/^ *id: "([^"]+)",.*/\1/p' "$spa" 2>/dev/null || true)
[ -f "$spa" ] || fail "$spa: missing, so no Capability has an app"
for id in $(printf '%s\n' "${live[@]}" | sort -u); do
  [[ " ${appless[*]} " == *" $id "* ]] && continue
  grep -qxF "$id" <<<"$ids" || fail "$spa: the \`$id\` Capability has no entry (\`id: \"$id\"\`)"
done
for id in $ids; do
  printf '%s\n' "${live[@]}" | grep -qxF "$id" || fail "$spa: \`id: \"$id\"\` names no Capability with a registered Module"
done
for id in $(sort <<<"$ids" | uniq -d); do fail "$spa: \`id: \"$id\"\` is given twice"; done

areas=$(grep -c 'to: "' "$spa" 2>/dev/null || true)
needs=$(sed -nE 's/.*needs: \{ resource: "([^"]+)", action: "([^"]+)" \}.*/\1 \2/p' "$spa" 2>/dev/null || true)
[ "$areas" = "$(grep -c . <<<"$needs" || true)" ] ||
  fail "$spa: every Area needs \`needs: { resource: \"<resource>\", action: \"<action>\" }\`, on one line"
if [ -n "$needs" ]; then
  if declared=$(SQLX_OFFLINE=true cargo run --quiet --bin dump-spec -- --actions); then
    while read -r resource action; do
      grep -qxF "$resource $action" <<<"$declared" ||
        fail "$spa: an Area needs \`$resource\` / \`$action\`, which no registered Module's policies declare"
    done <<<"$needs"
  else
    fail "dump-spec --actions failed, so the Areas' needs aren't checked"
  fi
fi

duplicates=$(printf '%s\n' "${numbers[@]}" | sort -n | awk '$1 == prev { print $1 } { prev = $1 }' | sort -u)
for number in $duplicates; do
  fail "module number $number is used by: $(printf '%s\n' "${numbers[@]}" | awk -v n="$number" '$1 == n { print $2 }' | xargs)"
done

if [ "$failed" -ne 0 ]; then
  echo "module registration check failed; \`make new-module\` registers a new module everywhere" >&2
  exit 1
fi
echo "module registration ok"
