


#!/usr/bin/env bash
# --- Auto-create all /appdata bind-mount directories from docker-compose.yml ---
COMPOSE_FILE="$(dirname "$0")/docker-compose.yml"
if [ -f "$COMPOSE_FILE" ]; then
	grep '^[[:space:]]*-[[:space:]]*/appdata' "$COMPOSE_FILE" | cut -d: -f1 | sed 's/^[[:space:]]*-[[:space:]]*//' | while read DIR; do
		if [ ! -d "$DIR" ]; then
			mkdir -p "$DIR"
			echo "[pre-sync] Aangemaakt: $DIR"
		fi
	done
fi
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
