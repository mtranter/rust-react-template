#!/usr/bin/env bash
# Cargo's rustc-wrapper (.cargo/config.toml): compiles through sccache when one
# is installed, else plain rustc, so a fresh clone builds before `make setup`.
# A git worktree has no .bin of its own (gitignored), so the main checkout's
# is tried too: every worktree then shares one cache.
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
main=$(dirname "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)")
for sccache in "$root"/.bin/*/sccache/*/bin/sccache "$main"/.bin/*/sccache/*/bin/sccache "$(command -v sccache)"; do
  [ -x "$sccache" ] && exec "$sccache" "$@"
done
exec "$@"
