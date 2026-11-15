#!/usr/bin/env bash
# Scaffolds a read module keeping its own copy of another module's roots, from
# their topic (ADR 0036), from crates/projection-template — `widget-index`,
# projecting the module template's `widgets` — and registers it everywhere
# scripts/check-modules.sh looks. Its tests pass as generated.
#
#   scripts/new-projection.sh <name> <provider> [<topic>]    e.g. `note-index notes`
#
# <provider> is a write module under crates/modules with a seed; <topic> names
# one of the topics its `-api` declares (`NoteTopic`), needed only when it
# declares several. Needs the dev database, as scripts/new-module.sh does.
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
  echo "new-projection: $1" >&2
  exit 1
}

name=${1:-}
provider=${2:-}
[ -n "$name" ] && [ -n "$provider" ] || die "usage: scripts/new-projection.sh <name> <provider> [<topic>]"
[[ $name =~ ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ ]] || die "\`$name\`: use lowercase words joined by '-'"
[ "$name" != widget-index ] || die "\`widget-index\` is the projection template's own name: pick another"
[ ! -e "crates/modules/$name" ] || die "crates/modules/$name already exists"
! grep -qE "^$name = " Cargo.toml || die "the workspace already has a crate named \`$name\`"
events=crates/modules/$provider/api/src/events.rs
[ -f "$events" ] || die "$events not found: <provider> is a write module with an \`-api\` crate"
[ -f "crates/modules/$provider/core/src/seed.rs" ] || die "\`$provider\` has no seed for the generated tests to announce"

topics=$(sed -nE 's/.*pub topic ([A-Za-z0-9]+)\(.*/\1/p' "$events")
topic=${3:-}
if [ -z "$topic" ]; then
  [ "$(grep -c . <<<"$topics")" -eq 1 ] || die "\`$provider\` declares several topics ($(echo $topics)): name one"
  topic=$topics
fi
grep -qxF "$topic" <<<"$topics" || die "\`$provider\` declares no topic \`$topic\` (it declares: $(echo $topics))"
# The topic's event enum is named on the `events:` line of its `topic!` block.
event_enum=$(awk -v t="$topic" '$0 ~ "pub topic "t"\\(" { found = 1 } found && /^ *events:/ { sub(/.*events: */, ""); sub(/,.*/, ""); print; exit }' "$events")
[ -n "$event_enum" ] || die "can't find \`$topic\`'s event enum in $events"
deletion=$(awk -v t="$topic" '$0 ~ "pub topic "t"\\(" { found = 1 } found && /=> Deleted:/ { print "Deleted"; exit } found && /^ *}/ { exit }' "$events")

DATABASE_URL=${DATABASE_URL:-postgres://app:app@localhost:5432/app}
export DATABASE_URL

pascal() { perl -pe 's/(?:^|-)([a-z0-9])/\U$1/g' <<<"$1"; }
ident=${name//-/_}
provider_ident=${provider//-/_}
existing=$(sed -nE 's/^name = "([^"]+)"/\1/p' crates/modules/*/core/Cargo.toml | paste -sd '|' -)

make --no-print-directory migrate

dir=crates/modules/$name/core
template=crates/projection-template/core
while IFS= read -r -d '' source; do
  path=${source#"$template"/}
  path=${path/#tests\/widget_index\//tests/$ident/}
  mkdir -p "$(dirname "$dir/$path")"
  # One pass, so a replacement is never itself replaced.
  NAME=$name IDENT=$ident PASCAL=$(pascal "$name") PROVIDER=$provider \
    PROVIDER_IDENT=$provider_ident TOPIC=$topic EVENT=$event_enum perl -pe '
      s{^widgets-api = \{ path = "[^"]*" \}}{$ENV{PROVIDER}-api.workspace = true};
      s{^widgets = \{ path = "[^"]*", features = \["seed"\] \}}{$ENV{PROVIDER} = \{ workspace = true, features = ["seed"] \}};
      BEGIN {
        %to = ("widget-index" => $ENV{NAME}, "widget_index" => $ENV{IDENT}, "WidgetIndex" => $ENV{PASCAL},
               "widgets_api" => "$ENV{PROVIDER_IDENT}_api", "widgets::" => "$ENV{PROVIDER_IDENT}::",
               "WidgetTopic" => $ENV{TOPIC}, "WidgetEvent" => $ENV{EVENT}, "widget" => $ENV{PROVIDER});
      }
      s{(widget-index|widget_index|WidgetIndex|widgets_api|widgets::|WidgetTopic|WidgetEvent|widget\b)}{$to{$1}}g;
    ' "$source" >"$dir/$path"
done < <(find "$template" -type f -print0)

if [ -n "$deletion" ]; then
  EVENT=$event_enum perl -0 -i -pe '
    my $e = $ENV{EVENT};
    s|fn removes\(&self, _event: &(\w+)\) -> bool \{\n\s*false\n|fn removes(&self, event: &$1) -> bool {\n        matches!(event, ${e}::Deleted)\n|s;
  ' "$dir/src/db/projection.rs"
fi

after_last() {
  local file=$1 pattern=$2 line=$3 at
  at=$(grep -nE "$pattern" "$file" | tail -1 | cut -d: -f1)
  [ -n "$at" ] || die "$file: nothing matches /$pattern/ to add \`$line\` after"
  awk -v at="$at" -v line="$line" '{ print } NR == at { print line }' "$file" >"$file.new"
  mv "$file.new" "$file"
}

after_last Cargo.toml '^[a-z0-9-]+ = \{ path = "crates/modules/' "$name = { path = \"$dir\" }"
after_last crates/server/Cargo.toml "^($existing)\.workspace = true" "$name.workspace = true"
NAME=$name perl -i -pe 's{(\{ crate = "(?:sqlx|poem)", wrappers = \[[^\]]*)\]}{$1, "$ENV{NAME}"]}g' deny.toml
ENDPOINTS=$ident::$(pascal "$name")Endpoints MODULE=$ident::module\(\) perl -0 -i -pe '
  s{(pub type Endpoints = \(.*?)(,?\s*\);)}{$1, $ENV{ENDPOINTS}$2}s;
  s{(pub fn modules\(\) -> Vec<Module> \{\s*vec!\[.*?)(,?\s*\]\s*\n\})}{$1, $ENV{MODULE}$2}s;
' crates/server/src/modules.rs
TOOLS=$(sed -nE 's/.*operation_id = "([^"]+)".*/    "\1",/p' "$dir/src/web/handlers.rs") perl -0 -i -pe '
  s{(const TOOLS: &\[&str\] = &\[.*?\n)(\];)}{$1$ENV{TOOLS}\n$2}s;
' crates/server/tests/composed_app.rs
cargo fmt --all

./scripts/migrate-schema.sh crates/module-template/core widgets
./scripts/migrate-schema.sh crates/projection-template/core widget_index
./scripts/migrate-schema.sh "$dir" "$ident"
SQLX_OFFLINE=false cargo bin sqlx prepare --workspace -- --all-targets

./scripts/check-modules.sh

removal="it removes a root's row on \`$event_enum::Deleted\`"
[ -n "$deletion" ] || removal="\`$topic\` has no deletion, so \`removes\` in $dir/src/db/projection.rs says false: change it if one is added"
cat <<EOF

Created $dir, a read module keeping a copy of \`$provider\`'s \`$topic\` from the earliest,
and registered it in crates/server/src/modules.rs, crates/server/Cargo.toml, Cargo.toml and
deny.toml. Its operation is an MCP tool, listed in crates/server/tests/composed_app.rs; $removal.

  make test-backend   its tests, and the composed-app test, pass as generated
  make generate       adds its operation to the frontend client

Then make it yours: replace the \`entity\` column with the typed columns you list, search or
sort by, in its migration, \`db/projection.rs\` and \`db/finder.rs\`, and run \`make prepare\`.
A later change of shape is a new generation: new tables and \`_v2\` queues, listed first in
the finder's \`Generations\` (see \`note-search\`).
EOF
