


#!/usr/bin/env bash
# Source INFISICAL_ variables if present
if [ -f /root/.env ]; then
	set -a
	source /root/.env
	set +a
fi
set -euo pipefail
# Maak het gedeelde netwerk aan als het nog niet bestaat
docker network create paperless_network 2>/dev/null || true

# Generate .env for ai-assistant
infisical export --env=prod --path=paperless/ai-assistant/.env > /appdata/paperless/ai-assistant/.env
# Generate .env for db
infisical export --env=prod --path=paperless/db/.env > /appdata/paperless/db/.env
# Generate .env for webserver
infisical export --env=prod --path=paperless/webserver/.env > /appdata/paperless/webserver/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/paperless/promtail/.env