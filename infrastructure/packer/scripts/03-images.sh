#!/usr/bin/env bash
# Pulls the repo's compose images, the observability profile's included, so
# `make dev` on a fresh box starts without a download.
set -euo pipefail

sudo docker compose -f /tmp/docker-compose.yml --profile observability pull
rm /tmp/docker-compose.yml
