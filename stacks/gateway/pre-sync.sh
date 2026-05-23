#!/usr/bin/env bash
# Script Name: pre-sync.sh
# Description: Pre-sync hook for the gateway stack. Automatically executed by node-sync.sh.

set -euo pipefail

NETWORK_NAME="gateway_network"

echo "[pre-sync] Verifying Docker network: ${NETWORK_NAME}"

if ! docker network inspect "${NETWORK_NAME}" >/dev/null 2>&1; then
    echo "[pre-sync] Network '${NETWORK_NAME}' not found. Creating it now..."
    docker network create "${NETWORK_NAME}"
    echo "[pre-sync] Network '${NETWORK_NAME}' created successfully."
else
    echo "[pre-sync] Network '${NETWORK_NAME}' already exists. Skipping."
fi

# Ensure required directories exist
mkdir -p /appdata/gateway/crowdsec
mkdir -p /appdata/gateway/goaccess
mkdir -p /appdata/gateway/nginx-proxy-manager
mkdir -p /appdata/gateway/promtail

# Generate .env for crowdsec
infisical export --env=prod --path=gateway/crowdsec/.env > /appdata/gateway/crowdsec/.env
# Generate .env for goaccess
infisical export --env=prod --path=gateway/goaccess/.env > /appdata/gateway/goaccess/.env
# Generate .env for nginx-proxy-manager
infisical export --env=prod --path=gateway/nginx-proxy-manager/.env > /appdata/gateway/nginx-proxy-manager/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/gateway/promtail/.env
