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

# Load credentials from .env — same lookup order as bootstrap-lxc.sh.
# This is required so git fetch can authenticate without prompting,
# both when run manually and from cron.
if [[ -f "${REPO_DIR}/.env" ]]; then
    set -a; source "${REPO_DIR}/.env"; set +a
elif [[ -f "${REPO_DIR}/scripts/host/.env" ]]; then
    set -a; source "${REPO_DIR}/scripts/host/.env"; set +a
fi

GITHUB_USERNAME="${GITHUB_USERNAME:-}"
GITHUB_PAT="${GITHUB_PAT:-}"

cd "${REPO_DIR}" || exit 1

# If credentials are available, embed them in the remote URL so every fetch
# authenticates automatically. This also picks up PAT rotations on each run —
# the stored URL is always derived from the current .env value.
if [[ -n "$GITHUB_USERNAME" && -n "$GITHUB_PAT" ]]; then
    # Strip any previously embedded credentials before re-embedding to avoid
    # accumulating user:pass@ segments on repeated runs.
    BARE_URL=$(git remote get-url origin | sed 's|https://[^@]*@|https://|')
    git remote set-url origin "${BARE_URL/https:\/\//https:\/\/${GITHUB_USERNAME}:${GITHUB_PAT}@}"
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
