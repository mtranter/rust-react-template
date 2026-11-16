# Builds share one sccache per machine, through a repo-local wrapper

Builds on [ADR 0027](0027-rust-cli-tools-are-repo-local.md). Agents work in parallel git worktrees, each with its own `target/`, so each one's first build compiles every dependency again: about 40s of the cold build and tests on an 18-core Mac, almost all of it dependencies.

**`.cargo/config.toml` sets `build.rustc-wrapper` to `scripts/rustc-wrapper.sh`.** The script runs rustc through sccache when it finds one: the checkout's `.bin` first, then the main checkout's (a worktree has no `.bin` of its own, since it is gitignored), then `PATH`. Without one it runs rustc directly, so a fresh clone builds before `make setup`, and CI, which doesn't install it, is unchanged. sccache is pinned in `[workspace.metadata.bin]` like the other tools.

Every checkout on the machine shares sccache's one local cache, so a new worktree takes dependencies it already compiled from the cache: measured at 265 of 348 compiles hit, and a cold build and tests at 44s → 33s. What remains doesn't cache: the workspace's own crates (incremental), proc macros, binaries, and linking.

## Considered options

- **`rustc-wrapper = "sccache"` directly.** Every `cargo` command fails until sccache is installed, including the `cargo install` behind `make setup` that installs it.
- **Each developer configures sccache in `~/.cargo/config.toml`.** Nothing travels with the repo, and an agent's machine has whatever it was given.
- **One `CARGO_TARGET_DIR` shared by every worktree.** Cargo's build lock serialises the parallel agents, and their different versions of the workspace's crates rebuild over each other.

## Consequences

- sccache's Rust cache key includes the compile's `CARGO_*` environment, so a build with a different `CARGO_TARGET_DIR` misses the cache. Its `basedirs` setting doesn't change that for Rust.
- `make setup` installs sccache once per machine, from source where no prebuilt binary is fetched (about 3 minutes).
