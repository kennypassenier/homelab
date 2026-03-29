#!/usr/bin/env bash
# Script Name: setup-host-cron.sh
# Description: Automates the setup of the cron job for host-sync.sh on the Proxmox host.
# Usage: ./setup-host-cron.sh [/path/to/homelab]

set -euo pipefail

REPO_DIR="${1:-/root/homelab}"
SYNC_SCRIPT="${REPO_DIR}/scripts/host/host-sync.sh"
LOG_FILE="/var/log/host-sync.log"

# Define the full cron command
# Runs at the top of every hour
CRON_SCHEDULE="0 * * * *"
CRON_CMD="${CRON_SCHEDULE} ${SYNC_SCRIPT} ${REPO_DIR} >> ${LOG_FILE} 2>&1"

echo "Verifying host-sync script exists at: ${SYNC_SCRIPT}"
if [[ ! -f "${SYNC_SCRIPT}" ]]; then
    echo "Error: ${SYNC_SCRIPT} not found."
    echo "Please ensure the repository is correctly cloned at ${REPO_DIR}."
    exit 1
fi

# Ensure the script is executable
chmod +x "${SYNC_SCRIPT}"

# Check if the crontab already contains the script
if crontab -l 2>/dev/null | grep -q "${SYNC_SCRIPT}"; then
    echo "Cron job for host-sync.sh is already configured in the crontab. Skipping."
else
    echo "Adding host-sync.sh to crontab to run every hour..."
    # Append the new cron job while preserving existing ones
    (crontab -l 2>/dev/null || true; echo "${CRON_CMD}") | crontab -
    echo "Cron job successfully added."
    echo "Logs will be written to ${LOG_FILE}."
fi
