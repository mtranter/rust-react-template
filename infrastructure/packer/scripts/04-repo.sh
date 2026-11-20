#!/usr/bin/env bash
# Optional: bakes a clone of the app with its repo-local tools installed and
# sccache warm (ADR 0037), so a box's first `make check-changed` skips the
# dependency compiles. Its `target/` is dropped; sccache's cache is what lasts.
set -euo pipefail

[ -n "$REPO_URL" ] || { echo "no repo_url: skipping the warm clone"; exit 0; }

. "$HOME/.cargo/env"
export PATH="$HOME/.local/bin:$PATH"

git clone --branch "$REPO_REF" "$REPO_URL" "$HOME/app"
cd "$HOME/app"
eval "$(mise env -s bash)"

make setup
(cd frontend && pnpm install --frozen-lockfile)
SQLX_OFFLINE=true cargo build --workspace --all-targets --features server/seed
.bin/*/sccache/*/bin/sccache --show-stats 2>/dev/null || true
.bin/*/sccache/*/bin/sccache --stop-server 2>/dev/null || true
rm -rf target
