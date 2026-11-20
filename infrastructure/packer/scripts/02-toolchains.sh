#!/usr/bin/env bash
# Runs as ubuntu, the box's developer.
set -euo pipefail

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain stable --component rustfmt,clippy
. "$HOME/.cargo/env"
rustup toolchain install "$RUST_VERSION" --profile minimal --component rustfmt,clippy

curl -L --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
cargo binstall -y cargo-run-bin

curl -fsSL https://mise.run | sh
export PATH="$HOME/.local/bin:$PATH"
cat >> "$HOME/.bashrc" <<'EOF'
eval "$(~/.local/bin/mise activate bash)"
EOF
mise use -g "node@$NODE_VERSION" "pnpm@$PNPM_VERSION"
eval "$(mise env -s bash)"

sudo env "PATH=$PATH" npx -y "playwright@$PLAYWRIGHT_VERSION" install-deps chromium
npx -y "playwright@$PLAYWRIGHT_VERSION" install chromium

if [ "$INSTALL_CLAUDE_CODE" = "true" ]; then
  curl -fsSL https://claude.ai/install.sh | bash
fi
