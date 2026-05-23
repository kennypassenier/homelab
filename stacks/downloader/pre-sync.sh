


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

# Generate INFISICAL_TOKEN for machine identity
export INFISICAL_TOKEN=$(infisical login --method=universal-auth \
  --client-id="$INFISICAL_CLIENT_ID" \
  --client-secret="$INFISICAL_CLIENT_SECRET" \
  --plain --silent)

# Generate .env for qbittorrent (export to stack dir for compose)
infisical export --token="$INFISICAL_TOKEN" --projectId="$INFISICAL_PROJECT_ID" --env=prod --path=downloader/qbittorrent > /opt/gitops/stacks/downloader/qbittorrent/.env
# Generate .env for promtail from shared/promtail (export to stack dir for compose)
infisical export --token="$INFISICAL_TOKEN" --projectId="$INFISICAL_PROJECT_ID" --env=prod --path=shared/promtail > /opt/gitops/stacks/downloader/promtail/.env
