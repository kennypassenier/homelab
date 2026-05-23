export $(cat /proc/1/environ | tr '\0' '\n' | grep '^INFISICAL_' | xargs)
#!/usr/bin/env bash
set -euo pipefail
# Ensure required directories exist
mkdir -p /appdata/downloader/qbittorrent/config
mkdir -p /appdata/downloader/promtail

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
