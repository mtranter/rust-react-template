---
id: 0064
name: "rig and Bedrock behind the assistant's model seam, with the MSRV raised to 1.97.1"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0063-the-assistant-platform-module-exists-and-reports-w.md]
blocks: [docs/platform/tickets/0066-sending-a-message-runs-a-streamed-turn-that-reads.md]
---

# rig and Bedrock behind the assistant's model seam, with the MSRV raised to 1.97.1

Add `rig-core` and `rig-bedrock` 0.46 to the workspace dependencies and use them in `assistant` behind one seam (a trait or rig's `CompletionModel`): a Bedrock implementation built from `APP_AGENT_MODEL` and the AWS SDK's default credential chain, and a scripted fake model for tests that returns given text and tool calls, wired in through the module's driver. rig's types stay behind the seam. This ticket carries the build fallout: the MSRV rise and the AWS SDK's `default-https-client` (rustls on `aws-lc-rs`).

## Acceptance criteria
- [ ] `rig-core` and `rig-bedrock` 0.46 are workspace dependencies used by `assistant`; no other crate depends on rig.
- [ ] Workspace `rust-version` is 1.97.1, and the toolchain comment, CI, `mise.toml`, the Packer `rust_version` and ADR 0026's pin note match.
- [ ] The Cargo.toml comments and ADR 0005 that say aws-lc-rs/cmake are kept out are corrected to say it is accepted for Bedrock; `deny.toml` allows the licences the new tree brings and `cargo deny` passes.
- [ ] The Dockerfile builds (cmake or a C toolchain added to the build image if `aws-lc-sys` needs it), CI builds, and the Graviton dev box build is checked (Packer/setup updated if needed).
- [ ] The Bedrock model is constructed only when `APP_AGENT_MODEL` is set; no test or CI step calls Bedrock.
- [ ] The scripted fake model is available to the module's tests (and to journeys) through the driver; a test shows it scripted with text and a tool call.
- [ ] The `run-and-deploy` skill lists the `APP_AGENT_*` variables, the AWS region/credential chain, the IAM permissions (`bedrock:InvokeModelWithResponseStream`, `bedrock:InvokeModel`), and that tool results are sent to the configured model (choose region and model for data residency).
- [ ] `make check-changed` passes.
