#!/usr/bin/env bash
# Script Name: host-sync.sh
# Description: Synchronizes the Proxmox host's local clone of the homelab repository.
# Can be run manually or via cron:
# 0 * * * * /root/homelab/scripts/host/host-sync.sh /root/homelab >> /var/log/host-sync.log 2>&1

set -euo pipefail

# Default location for the repo on the Proxmox host
REPO_DIR="${1:-/root/homelab}"

if [[ ! -d "${REPO_DIR}/.git" ]]; then
    echo "[$(date -Iseconds)] Error: ${REPO_DIR} is not a valid Git repository."
    exit 1
fi

echo "[$(date -Iseconds)] Starting Proxmox host repository sync..."

cd "${REPO_DIR}" || exit 1

# Fetch and enforce the latest state from the main branch
git fetch origin main --quiet
git reset --hard origin/main --quiet

# Ensure all scripts in the host directory remain executable
if [[ -d "scripts/host" ]]; then
    chmod +x scripts/host/*.sh
    echo "[$(date -Iseconds)] Permissions verified for host scripts."
fi

echo "[$(date -Iseconds)] Host sync completed successfully."
