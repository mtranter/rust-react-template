#!/usr/bin/env bash
# Scaffolds a write module, its `core` and `api` crates, from
# crates/module-template — a module named `widgets` whose aggregate is a
# `widget` — in its Capability's folder, crates/capabilities/<capability>/
# (ADR 0047), and registers it everywhere scripts/check-modules.sh looks. Its
# tests pass as generated.
#
#   scripts/new-module.sh <capability> <name> [<entity>]    e.g. `sales order-lines order-line`
#
# The first Module of a Capability also makes the Capability: its folder and
# `capability.yaml` (its label the id in words), the workspace's
# `crates/capabilities/` members if it is the first, and its app in the SPA: an
# entry in frontend/src/shell/capabilities.ts with one Area, the Module's,
# routed to a page saying it isn't built yet.
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

capability=${1:-}
name=${2:-}
entity=${3:-${name%s}}
[ -n "$capability" ] && [ -n "$name" ] || die "usage: scripts/new-module.sh <capability> <name> [<entity>]"
[[ $capability =~ ^[a-z][a-z0-9-]*$ ]] ||
  die "\`$capability\`: a Capability's id is lowercase letters, digits and '-', from a letter"
for word in "$name" "$entity"; do
  [[ $word =~ ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ ]] || die "\`$word\`: use lowercase words joined by '-'"
done
[ "$entity" != "$name" ] || die "the entity must differ from the module name: pass it, e.g. \`$name $name-item\`"
[ "$name" != widgets ] || die "\`widgets\` is the module template's own name: pick another"
for taken in crates/capabilities/*/"$name" "crates/platform/$name" "crates/reference/$name"; do
  [ ! -e "$taken" ] || die "$taken already exists: a Module's name is taken in every Capability, the platform and the reference"
done
! grep -qE "^$name = " Cargo.toml || die "the workspace already has a crate named \`$name\`"

capability_dir=crates/capabilities/$capability
capability_file=$capability_dir/capability.yaml
for other in crates/platform/*/capability.yaml crates/reference/capability.yaml crates/capabilities/*/capability.yaml; do
  [ "$other" = "$capability_file" ] && continue
  ! grep -qxE "id: $capability" "$other" || die "\`$capability\` is already the id of the Capability in $(dirname "$other")"
done
new_capability=false
[ -f "$capability_file" ] || new_capability=true

DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}
export DATABASE_URL

pascal() { perl -pe 's/(?:^|-)([a-z0-9])/\U$1/g' <<<"$1"; }
camel() { perl -pe 's/-([a-z0-9])/\U$1/g' <<<"$1"; }
in_words() { perl -pe 's/-/ /g; s/^(.)/\U$1/' <<<"$1"; }
ident=${name//-/_}
entity_ident=${entity//-/_}
entity_upper=$(tr '[:lower:]' '[:upper:]' <<<"$entity_ident")
plural_type=$(pascal "$name")
entity_type=$(pascal "$entity")

# The Capability's app, when it is new.
capability_label=$(in_words "$capability")
area_label=$(in_words "$name")
registry=frontend/src/shell/capabilities.ts
router=frontend/src/router.tsx
page=frontend/src/pages/$capability-$name-page.tsx
page_component=$(pascal "$capability")${plural_type}Page
route=$(camel "$capability")${plural_type}Route
capability_const=$(camel "$capability")
if $new_capability; then
  [ ! -e "$page" ] || die "$page already exists"
  ! grep -qE "\b($route|$capability_const)\b" "$router" "$registry" ||
    die "\`$route\` or \`$capability_const\` is already named in $router or $registry"
fi

cores=(crates/capabilities/*/*/core/Cargo.toml crates/platform/*/core/Cargo.toml crates/reference/*/core/Cargo.toml)
existing=$( ((${#cores[@]})) && sed -nE 's/^name = "([^"]+)"/\1/p' "${cores[@]}" | paste -sd '|' - || true)
# The platform's modules number from 1000, clear of the app's, so they don't count here.
seeds=(crates/capabilities/*/*/*/src/seed.rs crates/reference/*/*/src/seed.rs)
last=$( ((${#seeds[@]})) && sed -nE 's/^pub const MODULE: u32 = ([0-9]+);/\1/p' "${seeds[@]}" | sort -n | tail -1 || true)
number=$((${last:-0} + 1))

# The `.sqlx` refresh below compiles every crate against the dev database, so
# the existing modules' schemas must be current there too.
make --no-print-directory migrate

if $new_capability; then
  mkdir -p "$capability_dir"
  printf 'id: %s\nlabel: %s\n' "$capability" "$capability_label" >"$capability_file"
fi
if ! grep -qF '"crates/capabilities/*/*/core"' Cargo.toml; then
  perl -0 -i -pe 's{(\n    "crates/\*/\*/api",\n)}{$1    # The app'"'"'s Modules, each in its Capability'"'"'s folder (ADR 0047). `make new-module`\n    # adds these with the first Capability: a glob matching nothing fails the workspace.\n    "crates/capabilities/*/*/core",\n    "crates/capabilities/*/*/api",\n}' Cargo.toml
  grep -qF '"crates/capabilities/*/*/core"' Cargo.toml ||
    die "Cargo.toml: no \`\"crates/*/*/api\",\` member to add the Capabilities' members after"
fi

root=$capability_dir/$name
dir=$root/core
template=crates/module-template
while IFS= read -r -d '' source; do
  path=${source#"$template"/}
  # The template's stands in for a Capability's; the Module names its folder's.
  [ "$path" = capability.yaml ] && continue
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
      s{policy_file!\("\.\./capability\.yaml"\)}{policy_file!("../../capability.yaml")};
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

after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/(capabilities|platform|reference)/' "$name-api = { path = \"$root/api\" }"
after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/(capabilities|platform|reference)/' "$name = { path = \"$dir\" }"
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

if $new_capability; then
  cat >"$page" <<EOF
import { Page } from "@/components/page";

/** What \`make new-module\` made for $area_label, until its feature is built (frontend/AGENTS.md). */
export function $page_component() {
  return <Page title="$area_label" description="Not built yet." />;
}
EOF
  ROUTE=$route COMPONENT=$page_component PAGE="@/pages/$capability-$name-page" TO="/$capability/$name" perl -0 -i -pe '
    s{\A}{import { $ENV{COMPONENT} } from "$ENV{PAGE}";\n};
    s{(\nconst routeTree = )}{\nconst $ENV{ROUTE} = createRoute({\n  getParentRoute: () => appRoute,\n  path: "$ENV{TO}",\n  component: $ENV{COMPONENT},\n});\n$1} or die "no routeTree\n";
    s{(appRoute\.addChildren\(\[\n\s*indexRoute,\n)}{$1    $ENV{ROUTE},\n} or die "no indexRoute among appRoute'"'"'s children\n";
  ' "$router"
  CONST=$capability_const ID=$capability LABEL=$capability_label AREA=$area_label \
    PATH_TO="/$capability" TO="/$capability/$name" perl -0 -i -pe '
    s{\A}{import { BoxIcon, ListIcon } from "lucide-react";\n};
    s{(\n/\*\*\n \* The apps the caller may open)}{\nconst $ENV{CONST}: Capability = {\n  id: "$ENV{ID}",\n  label: "$ENV{LABEL}",\n  description: "What $ENV{LABEL} is for, in one line.",\n  icon: BoxIcon,\n  path: "$ENV{PATH_TO}",\n  areas: [{ label: "$ENV{AREA}", to: "$ENV{TO}", icon: ListIcon }],\n};\n$1} or die "no `useCapabilities` doc comment\n";
    s{(apps: \[)\]}{$1$ENV{CONST}]} or s{(apps: \[[^\]]+)\]}{$1, $ENV{CONST}]} or die "no `apps: [..]`\n";
  ' "$registry"
  # Biome sorts and merges the imports added above; without node_modules, `make lint-frontend` says how.
  if [ -d frontend/node_modules ]; then
    (cd frontend && pnpm exec biome check --write "${page#frontend/}" "${router#frontend/}" "${registry#frontend/}" >/dev/null)
  fi
fi

# `sqlx prepare` checks every crate's queries against the dev database, the
# template's, the reference modules' and the new module's included.
./scripts/migrate-unregistered.sh
./scripts/migrate-schema.sh "$dir" "$ident"
SQLX_OFFLINE=false cargo bin sqlx prepare --workspace -- --all-targets

./scripts/check-modules.sh

app=
$new_capability && app="

The Capability is new: $capability_file, and its app in $registry, with one Area,
$area_label, routed in $router to $page. Describe the app there, and build the Area's feature."
cat <<EOF

Created $dir and $root/api (module number $number) in the \`$capability\` Capability, and
registered it in crates/server/src/modules.rs, crates/server/Cargo.toml, Cargo.toml and
deny.toml. Its operations are MCP tools, listed in crates/server/tests/composed_app.rs.$app

  make test-backend   its tests, and the composed-app test, pass as generated
  make generate       adds its operations to the frontend client

Then make it yours: reshape the \`$entity_type\` aggregate, keeping its migration, seed and
tests in step, and run \`make prepare\` after changing any \`query!\`. To use another module,
depend on its \`-api\` crate and pass the provider to \`$ident::module(..)\` in
crates/server/src/modules.rs, as crates/reference/journeys/src/lib.rs passes one.
EOF
