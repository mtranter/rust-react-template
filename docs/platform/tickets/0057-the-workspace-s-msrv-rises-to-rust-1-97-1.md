---
id: 0057
name: "The workspace's MSRV rises to Rust 1.97.1"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: []
blocks: [docs/platform/tickets/0058-an-assistant-platform-module-reports-whether-the-a.md]
---

# The workspace's MSRV rises to Rust 1.97.1

rig-bedrock 0.46 declares `rust-version = 1.97.1`, so before the `assistant` module can depend on it the workspace's minimum Rust version rises to 1.97.1 everywhere it is pinned. No behaviour changes; this is only the toolchain bump the Spec's "Dependencies and docs" section asks for, kept separate so a toolchain break is visible on its own.

## Acceptance criteria
- [ ] `Cargo.toml`'s `rust-version` is `1.97.1`, and its comment says why (rig-bedrock 0.46) instead of sqlx.
- [ ] The toolchain file and its comment, `mise.toml`, CI (`.github/workflows/ci.yml`) and the Packer `rust_version` all pin 1.97.1.
- [ ] ADR 0026's pin note is updated to 1.97.1.
- [ ] Any new clippy lints the new toolchain raises are fixed (not silenced wholesale).
- [ ] `make check-changed` passes.
