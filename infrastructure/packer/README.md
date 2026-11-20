# Dev box AMI

A Packer template for an arm64 (Graviton) Ubuntu 24.04 AMI with everything `make dev` and `make check` need, so that working on the app in AWS is: launch, clone, `make setup`, go.

## What's on it

- **Rust**: rustup with `stable` as its default and the pinned `rust_version`, which CI and `rust-toolchain.toml` also pin, plus `cargo-binstall` and `cargo-run-bin`, the bootstrap `make setup` needs (ADR 0027).
- **Node**: mise with the Node and pnpm from `mise.toml`, activated in `~/.bashrc`.
- **Docker**: Docker Engine with the compose and buildx plugins, with `ubuntu` in the `docker` group. Every image in `docker-compose.yml`, including the `observability` profile's, is already pulled.
- **Playwright**: Chromium and its system libraries, at the version in `frontend/package.json`, for `make e2e`.
- **CLIs**: git, gh, the AWS CLI v2 with the Session Manager plugin, psql, and Claude Code.
- **Tuning**: inotify limits raised for many worktrees' watchers. IMDSv2 is required, with a hop limit of 2 so containers can reach it.

If `repo_url` is set, the build also clones that app into `~/app`, runs `make setup` and `pnpm install`, and does a debug build. The build's `target/` is deleted but sccache's cache stays (ADR 0037), so the box's first build takes its dependency compiles from the cache.

## Build

```sh
cd infrastructure/packer
packer init .
AWS_REGION=ap-southeast-2 packer build .
# or with a warm clone of an app:
packer build -var region=ap-southeast-2 -var repo_url=https://github.com/<org>/<app>.git .
```

The build uses the credentials in your AWS environment (profile, SSO, or env vars) and the region's default VPC unless you set `vpc_id` and `subnet_id`. Build on a public subnet, since Packer connects over SSH. `manifest.json` records the AMI ID.

Every variable is listed in `variables.pkr.hcl`. The version pins `rust_version`, `node_version`, `pnpm_version` and `playwright_version` mirror CI, `mise.toml` and `frontend/package.json`. Bump them together.

## Launch

The AMI runs on any arm64 instance type. A `c7g.2xlarge` / `c8g.2xlarge` or larger keeps Rust builds reasonable. Sign in as `ubuntu` with the key pair you launch with, or through SSM Session Manager if the instance has an instance profile with `AmazonSSMManagedInstanceCore`. To reach `make dev`'s ports, forward them over SSH:

```sh
ssh -L 5173:localhost:5173 -L 8080:localhost:8080 ubuntu@<host>
```
