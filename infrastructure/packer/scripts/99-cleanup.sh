#!/usr/bin/env bash
set -euo pipefail

apt-get -y autoremove
apt-get clean
rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*
rm -f /home/ubuntu/.ssh/authorized_keys /root/.ssh/authorized_keys
cloud-init clean --logs --machine-id
