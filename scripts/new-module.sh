#!/usr/bin/env bash
# Scaffolds a write module, its `core` and `api` crates, from
# crates/module-template — a module named `widgets` whose aggregate is a
# `widget` — and registers it everywhere scripts/check-modules.sh looks. Its
# tests pass as generated.
#
#   scripts/new-module.sh <name> [<entity>]    e.g. `order-lines order-line`
#
# <entity> defaults to <name> without a trailing `s`. Needs the dev database:
# the new module's `query!`s compile offline only once `.sqlx` knows them, and
# `sqlx prepare` checks them against its migrated schema.
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."

die() {
  echo "new-module: $1" >&2
  exit 1
}

name=${1:-}
entity=${2:-${name%s}}
[ -n "$name" ] || die "usage: scripts/new-module.sh <name> [<entity>]"
for word in "$name" "$entity"; do
  [[ $word =~ ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ ]] || die "\`$word\`: use lowercase words joined by '-'"
done
[ "$entity" != "$name" ] || die "the entity must differ from the module name: pass it, e.g. \`$name $name-item\`"
[ "$name" != widgets ] || die "\`widgets\` is the module template's own name: pick another"
for taken in "crates/modules/$name" "crates/platform/$name" "crates/reference/$name"; do
  [ ! -e "$taken" ] || die "$taken already exists"
done
! grep -qE "^$name = " Cargo.toml || die "the workspace already has a crate named \`$name\`"

DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}
export DATABASE_URL

pascal() { perl -pe 's/(?:^|-)([a-z0-9])/\U$1/g' <<<"$1"; }
ident=${name//-/_}
entity_ident=${entity//-/_}
entity_upper=$(tr '[:lower:]' '[:upper:]' <<<"$entity_ident")
plural_type=$(pascal "$name")
entity_type=$(pascal "$entity")

cores=(crates/modules/*/core/Cargo.toml crates/platform/*/core/Cargo.toml crates/reference/*/core/Cargo.toml)
existing=$( ((${#cores[@]})) && sed -nE 's/^name = "([^"]+)"/\1/p' "${cores[@]}" | paste -sd '|' - || true)
seeds=(crates/modules/*/*/src/seed.rs crates/platform/*/*/src/seed.rs crates/reference/*/*/src/seed.rs)
last=$( ((${#seeds[@]})) && sed -nE 's/^pub const MODULE: u32 = ([0-9]+);/\1/p' "${seeds[@]}" | sort -n | tail -1 || true)
number=$((${last:-0} + 1))

# The `.sqlx` refresh below compiles every crate against the dev database, so
# the existing modules' schemas must be current there too.
make --no-print-directory migrate

root=crates/modules/$name
dir=$root/core
template=crates/module-template
while IFS= read -r -d '' source; do
  path=${source#"$template"/}
  path=${path/#core\/tests\/widgets\//core/tests/$ident/}
  path=${path/#core\/src\/domain\/widget.rs/core/src/domain/$entity_ident.rs}
  path=${path/#core\/policies\/widget.yaml/core/policies/$entity_ident.yaml}
  crate=$ident
  [ "$(basename "$source")" = Cargo.toml ] && crate=$name
  mkdir -p "$(dirname "$root/$path")"
  # One pass, so a replacement is never itself replaced.
  URL=/$name CRATE=$crate PLURAL=$plural_type ENTITY=$entity_ident ENTITY_TYPE=$entity_type \
    ENTITY_UPPER=$entity_upper NUMBER=$number perl -pe '
      BEGIN {
        %to = ("/widgets" => $ENV{URL}, "Widgets" => $ENV{PLURAL}, "widgets" => $ENV{CRATE},
               "WIDGET" => $ENV{ENTITY_UPPER}, "Widget" => $ENV{ENTITY_TYPE}, "widget" => $ENV{ENTITY});
      }
      s{(/widgets|Widgets|widgets|WIDGET|Widget|widget)}{$to{$1}}g;
      s{^pub const MODULE: u32 = \d+;}{pub const MODULE: u32 = $ENV{NUMBER};};
    ' "$source" >"$root/$path"
done < <(find "$template" -type f -print0)

# Adds `line` after the last line of `file` matching `pattern`, else after the
# first matching `fallback`.
after_last() {
  local file=$1 pattern=$2 line=$3 fallback=${4:-} at
  at=$(grep -nE "$pattern" "$file" | tail -1 | cut -d: -f1 || true)
  [ -n "$at" ] || [ -z "$fallback" ] || at=$(grep -nE "$fallback" "$file" | head -1 | cut -d: -f1 || true)
  [ -n "$at" ] || die "$file: nothing matches /$pattern/ to add \`$line\` after"
  awk -v at="$at" -v line="$line" '{ print } NR == at { print line }' "$file" >"$file.new"
  mv "$file.new" "$file"
}

after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/(modules|platform|reference)/' "$name-api = { path = \"$root/api\" }"
after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/(modules|platform|reference)/' "$name = { path = \"$dir\" }"
after_last crates/server/Cargo.toml "^(${existing:-$name})\.workspace = true" "$name.workspace = true" '^\[dependencies\]'
NAME=$name perl -i -pe 's{^seed = \[\]}{seed = ["$ENV{NAME}/seed"]} or s{^(seed = \[.*)\]}{$1, "$ENV{NAME}/seed"]}' crates/server/Cargo.toml
NAME=$name perl -i -pe 's{(\{ crate = "(?:sqlx|poem)", wrappers = \[[^\]]*)\]}{$1, "$ENV{NAME}"]}g' deny.toml
# Each edit handles an empty list too, as `make init` leaves them. poem-openapi
# has no `OpenApi` for a 1-tuple, so one module's `Endpoints` aren't in one.
ENDPOINTS=$ident::${plural_type}Endpoints MODULE=$ident::module\(\) perl -0 -i -pe '
  s{(type Endpoints = )\(\);}{$1$ENV{ENDPOINTS};}
    or s{(type Endpoints = )([\w:]+);}{$1($2, $ENV{ENDPOINTS});}
    or s{(type Endpoints = \(.*?)(,?\s*\);)}{$1, $ENV{ENDPOINTS}$2}s;
  s{(fn modules\(\) -> Vec<Module> \{\s*vec!\[)\]}{$1$ENV{MODULE}]}
    or s{(fn modules\(\) -> Vec<Module> \{\s*vec!\[.*?)(,?\s*\]\s*\n\s*\})}{$1, $ENV{MODULE}$2}s;
' crates/server/src/modules.rs
# Its operations are tools at once; the composed-app test lists every tool.
TOOLS=$(sed -nE 's/.*operation_id = "([^"]+)".*/    "\1",/p' "$dir/src/web/handlers.rs") perl -0 -i -pe '
  s{(const TOOLS: &\[&str\] = &\[)\];}{$1\n$ENV{TOOLS}\n];}
    or s{(const TOOLS: &\[&str\] = &\[.*?\n)(\];)}{$1$ENV{TOOLS}\n$2}s;
' crates/server/tests/composed_app.rs
cargo fmt --all

# `sqlx prepare` checks every crate's queries against the dev database, the
# template's, the reference modules' and the new module's included.
./scripts/migrate-unregistered.sh
./scripts/migrate-schema.sh "$dir" "$ident"
SQLX_OFFLINE=false cargo bin sqlx prepare --workspace -- --all-targets

./scripts/check-modules.sh

cat <<EOF

Created $dir and $root/api (module number $number) and registered it in crates/server/src/modules.rs,
crates/server/Cargo.toml, Cargo.toml and deny.toml. Its operations are MCP tools, listed in
crates/server/tests/composed_app.rs.

  make test-backend   its tests, and the composed-app test, pass as generated
  make generate       adds its operations to the frontend client

Then make it yours: reshape the \`$entity_type\` aggregate, keeping its migration, seed and
tests in step, and run \`make prepare\` after changing any \`query!\`. To use another module,
depend on its \`-api\` crate and pass the capability to \`$ident::module(..)\` in
crates/server/src/modules.rs, as crates/reference/journeys/src/lib.rs passes one.
EOF
