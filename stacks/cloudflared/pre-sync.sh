


#!/usr/bin/env bash
# Source INFISICAL_ variables if present
if [ -f /root/.env ]; then
	set -a
	source /root/.env
	set +a
fi
set -euo pipefail

# Generate .env for cloudflared
infisical export --env=prod --path=cloudflared/cloudflared/.env > /appdata/cloudflared/cloudflared/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/cloudflared/promtail/.env
