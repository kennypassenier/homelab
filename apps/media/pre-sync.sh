#!/usr/bin/env bash
# Script Name: pre-sync.sh
# Description: Pre-sync hook for the media stack. Automatically executed by node-sync.sh.

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
