


#!/usr/bin/env bash
# Source INFISICAL_ variables if present
if [ -f /root/.env ]; then
  set -a
  source /root/.env
  set +a
fi
set -euo pipefail

# Only seed config if it doesn't exist
CONF="/appdata/downloader/qbittorrent/config/qBittorrent/qBittorrent.conf"
TEMPLATE="/appdata/downloader/qbittorrent.conf.template"
if [ ! -f "$CONF" ] && [ -f "$TEMPLATE" ]; then
  cp "$TEMPLATE" "$CONF"
fi

# Generate .env for qbittorrent
infisical export --env=prod --path=downloader/qbittorrent/.env > /appdata/downloader/qbittorrent/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/downloader/promtail/.env
