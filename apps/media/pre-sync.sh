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
