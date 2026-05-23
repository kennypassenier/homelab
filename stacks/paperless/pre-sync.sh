export $(cat /proc/1/environ | tr '\0' '\n' | grep '^INFISICAL_' | xargs)
#!/usr/bin/env bash
# Maak het gedeelde netwerk aan als het nog niet bestaat
docker network create paperless_network 2>/dev/null || true

# Ensure required directories exist
mkdir -p /appdata/paperless/ai-assistant
mkdir -p /appdata/paperless/db
mkdir -p /appdata/paperless/webserver
mkdir -p /appdata/paperless/promtail

# Generate .env for ai-assistant
infisical export --env=prod --path=paperless/ai-assistant/.env > /appdata/paperless/ai-assistant/.env
# Generate .env for db
infisical export --env=prod --path=paperless/db/.env > /appdata/paperless/db/.env
# Generate .env for webserver
infisical export --env=prod --path=paperless/webserver/.env > /appdata/paperless/webserver/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/paperless/promtail/.env