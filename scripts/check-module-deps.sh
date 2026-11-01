#!/usr/bin/env bash
# The Cargo graph isolates modules only while nobody adds the wrong line to a
# Cargo.toml (spec §4, D7, D8). From `cargo metadata`, this fails if:
#
#   - a module core's dependencies (normal or build) include another module core;
#     dev-dependencies may (a read module's tests run the providers' migrations
#     and seeds), and `server` isn't a module;
#   - an `-api` crate's workspace dependencies (normal or build) are anything but
#     pure crates: the pure `common` crates and other `-api` crates;
#   - a write module (one with `src/domain/`) declares `[package.metadata.module] reads`:
#     only a read module joins another module's views.
#
# The module template counts as a module core.
# A module's kind is inferred from its `src/domain/` folder: a read module has none.
set -euo pipefail
cd "$(dirname "$0")/.."

# Spec §5: the pure `common` crates. Never add a framework-coupled one.
pure_common='["app-domain", "app-error", "app-persistence", "app-test-support"]'

cargo metadata --no-deps --format-version 1 | jq -r --arg root "$PWD/" --argjson pure "$pure_common" '
  def rel: ltrimstr($root);
  def core: .manifest_path | rel | test("^(crates/modules/[^/]+/core|crates/module-template)/Cargo.toml$");
  def api: .manifest_path | rel | test("^crates/modules/[^/]+/api/Cargo.toml$");
  def shipped: .dependencies[] | select(.path and .kind != "dev");

  [.packages[] | select(core) | .name] as $cores
  | [.packages[] | select(api) | .name] as $apis
  | .packages[]
  | (.manifest_path | rel) as $manifest
  | if core then
      (shipped | select(.name as $d | $cores | index($d))
        | "\($manifest): depends on module core `\(.name)`; use `\(.name)-api` (spec D7)"),
      (if .metadata.module.reads then "\($manifest)\t\(.metadata.module.reads | join(", "))" else empty end)
    elif api then
      shipped | select(.name as $d | ($pure + $apis) | index($d) | not)
        | "\($manifest): `-api` crates stay pure, but it depends on `\(.name)` (spec §5, D7)"
    else empty end
' | {
  failed=0
  while IFS= read -r line; do
    if [[ "$line" == *$'\t'* ]]; then
      manifest=${line%%$'\t'*}
      [ -d "$(dirname "$manifest")/src/domain" ] || continue
      line="$manifest: a write module (it has src/domain/) declares reads = [${line#*$'\t'}]; only a read module joins other modules' views (spec D8)"
    fi
    echo "$line"
    failed=1
  done
  if [ "$failed" -ne 0 ]; then
    echo "module dependency check failed" >&2
    exit 1
  fi
}
echo "module dependencies ok"
