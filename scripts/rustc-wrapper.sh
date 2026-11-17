#!/usr/bin/env bash
# Cargo's rustc-wrapper (.cargo/config.toml): compiles through sccache when one
# is installed, else plain rustc, so a fresh clone builds before `make setup`.
# A git worktree has no .bin of its own (gitignored), so the main checkout's
# is tried too: every worktree then shares one cache.
# cargo-run-bin's shim (.bin/.shims/sccache, on PATH under `cargo bin ...`) is
# skipped: it runs `cargo bin sccache` from rustc's working directory, which for
# a registry crate is outside the workspace, so it fails ("No binaries
# configured") whenever sccache isn't installed into .bin yet.
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
main=$(dirname "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)")
IFS=$'\n' read -r -d '' -a on_path < <(type -ap sccache)
for sccache in "$root"/.bin/*/sccache/*/bin/sccache "$main"/.bin/*/sccache/*/bin/sccache "${on_path[@]}"; do
  case $sccache in */.bin/.shims/*) continue ;; esac
  [ -x "$sccache" ] && exec "$sccache" "$@"
done
exec "$@"
