# Rust CLI tools are repo-local, pinned in `Cargo.toml`

The Rust tools the build runs (`cargo-deny`, `cargo-watch`, `sqlx-cli`) are pinned in the root `Cargo.toml`'s `[workspace.metadata.bin]` and installed by `cargo-run-bin` into a gitignored `./.bin`, so every clone and CI run uses the same versions and nothing goes into `~/.cargo/bin` but the one bootstrap. `make setup` installs them; the Makefile runs each as `cargo bin <tool>`, never a global one. Frontend tools are already repo-local through `package.json`.

## Considered options

- **Global `cargo install`.** Versions drift between developers and CI, and a tool's behaviour (a `cargo-deny` check, the `.sqlx` format) with them.

## Consequences

- Bumping a tool is an edit to `[workspace.metadata.bin]` and `make setup`.
