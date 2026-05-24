

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

NETWORK_NAME="media_network"

echo "[pre-sync] Verifying Docker network: ${NETWORK_NAME}"

if ! docker network inspect "${NETWORK_NAME}" >/dev/null 2>&1; then
    echo "[pre-sync] Network '${NETWORK_NAME}' not found. Creating it now..."
    docker network create "${NETWORK_NAME}"
    echo "[pre-sync] Network '${NETWORK_NAME}' created successfully."
else
    echo "[pre-sync] Network '${NETWORK_NAME}' already exists. Skipping."
fi

# --- Jellyseerr to Seerr Migration ---
# This block is idempotent. Once the directory is renamed, it will be skipped in future runs.
if [ -d "/appdata/media/jellyseerr" ]; then
    echo "[pre-sync] Found legacy Jellyseerr data. Migrating to Seerr..."

    # Stop and remove the old container if it exists
    if docker ps -a --format '{{.Names}}' | grep -q "^jellyseerr$"; then
        echo "[pre-sync] Stopping and removing old jellyseerr container..."
        docker stop jellyseerr || true
        docker rm jellyseerr || true
    fi

    # Rename the data directory
    echo "[pre-sync] Renaming data directory from jellyseerr to seerr..."
    mv /appdata/media/jellyseerr /appdata/media/seerr

    echo "[pre-sync] Migration to Seerr complete."
fi

# Ensure proper permissions for Seerr (Fixes EACCES crash loop after migration)
if [ -d "/appdata/media/seerr" ]; then
    echo "[pre-sync] Ensuring correct ownership for Seerr data (1000:1000)..."
    chown -R 1000:1000 /appdata/media/seerr
fi

# Generate .env for jellyfin
infisical export --env=prod --path=media/jellyfin/.env > /appdata/media/jellyfin/.env

# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/media/promtail/.env
