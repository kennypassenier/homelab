#!/usr/bin/env bash
# Script Name: setup-cron.sh
# Description: Automates the setup of the cron job for sync-host.sh on the Proxmox host.
# Usage: ./setup-cron.sh [-d /path/to/homelab] [-h]

set -euo pipefail

show_help() {
    echo "Usage: $0 [-d <REPO_DIR>] [-h]"
    echo "  -d    Path to the homelab repository (default: /root/homelab)"
    echo "  -h    Show this help message"
    exit 0
}

REPO_DIR="/root/homelab"

while getopts "d:h" opt; do
    case "$opt" in
        d) REPO_DIR="$OPTARG" ;;
        h) show_help ;;
        *) show_help ;;
    esac
done
shift $((OPTIND-1))
SYNC_SCRIPT="${REPO_DIR}/scripts/host/sync-host.sh"
LOG_FILE="/var/log/host-sync.log"

# Define the full cron command
# Runs at the top of every hour
CRON_SCHEDULE="0 * * * *"
CRON_CMD="${CRON_SCHEDULE} ${SYNC_SCRIPT} ${REPO_DIR} >> ${LOG_FILE} 2>&1"

echo "Verifying sync-host script exists at: ${SYNC_SCRIPT}"
if [[ ! -f "${SYNC_SCRIPT}" ]]; then
    echo "Error: ${SYNC_SCRIPT} not found."
    echo "Please ensure the repository is correctly cloned at ${REPO_DIR}."
    exit 1
fi

# Ensure the script is executable
chmod +x "${SYNC_SCRIPT}"

# Check if the crontab already contains the script
if crontab -l 2>/dev/null | grep -q "${SYNC_SCRIPT}"; then
    echo "Cron job for sync-host.sh is already configured in the crontab. Skipping."
else
    echo "Adding sync-host.sh to crontab to run every hour..."
    # Append the new cron job while preserving existing ones
    (crontab -l 2>/dev/null || true; echo "${CRON_CMD}") | crontab -
    echo "Cron job successfully added."
    echo "Logs will be written to ${LOG_FILE}."
fi

# Configure logrotate for the host sync log so it never grows unbounded.
# Idempotent: writing the same config file again is harmless.
LOGROTATE_FILE="/etc/logrotate.d/host-sync"
cat > "${LOGROTATE_FILE}" <<EOF
${LOG_FILE} {
    daily
    rotate 7
    compress
    missingok
    notifempty
}
EOF
echo "Logrotate configured at ${LOGROTATE_FILE} (daily, 7 days retention)."
