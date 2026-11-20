#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

apt-get update
apt-get -y upgrade
apt-get install -y --no-install-recommends \
  build-essential pkg-config clang lld cmake \
  libssl-dev ca-certificates curl gnupg unzip zip \
  git make jq ripgrep fd-find tmux htop tree \
  postgresql-client

install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=arm64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
  > /etc/apt/sources.list.d/docker.list

curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /etc/apt/keyrings/githubcli.gpg
chmod a+r /etc/apt/keyrings/githubcli.gpg
echo "deb [arch=arm64 signed-by=/etc/apt/keyrings/githubcli.gpg] https://cli.github.com/packages stable main" \
  > /etc/apt/sources.list.d/github-cli.list

apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin gh
systemctl enable --now docker
usermod -aG docker ubuntu

tmp=$(mktemp -d)
curl -fsSL https://awscli.amazonaws.com/awscli-exe-linux-aarch64.zip -o "$tmp/awscliv2.zip"
unzip -q "$tmp/awscliv2.zip" -d "$tmp"
"$tmp/aws/install"
rm -rf "$tmp"

curl -fsSL https://s3.amazonaws.com/session-manager-downloads/plugin/latest/ubuntu_arm64/session-manager-plugin.deb -o /tmp/smp.deb
dpkg -i /tmp/smp.deb
rm /tmp/smp.deb

# Many worktrees each running cargo, vite and their watchers exhaust the defaults.
cat > /etc/sysctl.d/90-dev-box.conf <<'EOF'
fs.inotify.max_user_watches = 1048576
fs.inotify.max_user_instances = 1024
EOF
