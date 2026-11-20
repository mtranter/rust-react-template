# 0057: MSRV rises to Rust 1.97.1

- The Rust version is pinned in seven places that must move together: `Cargo.toml` `rust-version`, `rust-toolchain.toml` `channel`, `mise.toml` `rust`, the four `dtolnay/rust-toolchain@` steps in `.github/workflows/ci.yml`, the Packer `rust_version` default (`infrastructure/packer/variables.pkr.hcl`), the `Dockerfile` backend `FROM rust:` tag, and the prose in ADR 0026 and `crates/AGENTS.md`'s "Pinned" bullet. `grep -rn '<old version>'` excluding `target/`, `node_modules/`, tickets and learnings finds them.
- `rust-toolchain.toml` used to track `stable`; it now pins the MSRV so local clippy matches CI. A fresh machine needs `rustup toolchain install <ver>` (rustup does it on first cargo call), and the first build after a bump recompiles everything (new rustc, new sccache keys).
- `cargo clippy --all-features` fails without a built `frontend/dist` (the `embed` feature); use `make clippy`, which is what CI runs.
