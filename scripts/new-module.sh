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
[ ! -e "crates/modules/$name" ] || die "crates/modules/$name already exists"
! grep -qE "^$name = " Cargo.toml || die "the workspace already has a crate named \`$name\`"

DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}
export DATABASE_URL

pascal() { perl -pe 's/(?:^|-)([a-z0-9])/\U$1/g' <<<"$1"; }
ident=${name//-/_}
entity_ident=${entity//-/_}
entity_upper=$(tr '[:lower:]' '[:upper:]' <<<"$entity_ident")
plural_type=$(pascal "$name")
entity_type=$(pascal "$entity")

existing=$(sed -nE 's/^name = "([^"]+)"/\1/p' crates/modules/*/core/Cargo.toml | paste -sd '|' -)
last=$(sed -nE 's/^pub const MODULE: u32 = ([0-9]+);/\1/p' crates/modules/*/*/src/seed.rs | sort -n | tail -1)
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

after_last() {
  local file=$1 pattern=$2 line=$3 at
  at=$(grep -nE "$pattern" "$file" | tail -1 | cut -d: -f1)
  [ -n "$at" ] || die "$file: nothing matches /$pattern/ to add \`$line\` after"
  awk -v at="$at" -v line="$line" '{ print } NR == at { print line }' "$file" >"$file.new"
  mv "$file.new" "$file"
}

after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/modules/' "$name-api = { path = \"$root/api\" }"
after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/modules/' "$name = { path = \"$dir\" }"
after_last crates/server/Cargo.toml "^($existing)\.workspace = true" "$name.workspace = true"
NAME=$name perl -i -pe 's{^(seed = \[.*)\]}{$1, "$ENV{NAME}/seed"]}' crates/server/Cargo.toml
NAME=$name perl -i -pe 's{(\{ crate = "(?:sqlx|poem)", wrappers = \[[^\]]*)\]}{$1, "$ENV{NAME}"]}g' deny.toml
ENDPOINTS=$ident::${plural_type}Endpoints MODULE=$ident::module\(\) perl -0 -i -pe '
  s{(pub type Endpoints = \(.*?)(,?\s*\);)}{$1, $ENV{ENDPOINTS}$2}s;
  s{(pub fn modules\(\) -> Vec<Module> \{\s*vec!\[.*?)(,?\s*\]\s*\n\})}{$1, $ENV{MODULE}$2}s;
' crates/server/src/modules.rs
cargo fmt --all

# `sqlx prepare` checks every crate's queries against the dev database, the
# template's and the new module's included.
./scripts/migrate-schema.sh crates/module-template/core widgets
./scripts/migrate-schema.sh "$dir" "$ident"
SQLX_OFFLINE=false cargo bin sqlx prepare --workspace -- --all-targets

./scripts/check-modules.sh

cat <<EOF

Created $dir and $root/api (module number $number) and registered it in crates/server/src/modules.rs,
crates/server/Cargo.toml, Cargo.toml and deny.toml.

  make test-backend   its tests, and the composed-app test, pass as generated
  make generate       adds its operations to the frontend client

Then make it yours: reshape the \`$entity_type\` aggregate, keeping its migration, seed and
tests in step, and run \`make prepare\` after changing any \`query!\`. To use another module,
depend on its \`-api\` crate and pass the capability to \`$ident::module(..)\` in
crates/server/src/modules.rs (see \`notes::module(users::directory)\`).
EOF
