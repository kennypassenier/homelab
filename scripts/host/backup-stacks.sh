#!/usr/bin/env bash
# Script Name: proxmox-restic-backup.sh
# Description: Centralized Restic backup for the host drive. Pauses containers by label.

set -euo pipefail

# Load credentials from the host .env file. Secrets must never be hardcoded in scripts.
# The .env file should be at /root/homelab/scripts/host/.env (chmod 600, not committed to Git).
ENV_FILE="$(dirname "$0")/.env"
if [[ -f "$ENV_FILE" ]]; then
    chmod 600 "$ENV_FILE"
    set -a
    # shellcheck source=/dev/null
    source "$ENV_FILE"
    set +a
fi

# Fail loudly if required secrets are missing rather than running with wrong/empty values.
if [[ -z "${RESTIC_PASSWORD:-}" ]]; then
    echo "ERROR: RESTIC_PASSWORD is not set. Add it to scripts/host/.env on the Proxmox host."
    exit 1
fi
if [[ -z "${RESTIC_REPOSITORY:-}" ]]; then
    echo "ERROR: RESTIC_REPOSITORY is not set. Add it to scripts/host/.env on the Proxmox host."
    exit 1
fi

if [[ "${RESTIC_REPOSITORY}" == rclone:* ]]; then
    if ! command -v rclone >/dev/null 2>&1; then
        echo "ERROR: RESTIC_REPOSITORY uses rclone backend but 'rclone' is not installed."
        exit 1
    fi

    if [[ -n "${RCLONE_CONFIG_FILE:-}" && ! -f "${RCLONE_CONFIG_FILE}" ]]; then
        echo "ERROR: RCLONE_CONFIG_FILE is set but file does not exist: ${RCLONE_CONFIG_FILE}"
        exit 1
    fi
fi

APPDATA_DIR="/opt/appdata"
export RESTIC_PASSWORD RESTIC_REPOSITORY

ensure_restic_repo() {
    if restic snapshots >/dev/null 2>&1; then
        return 0
    fi

    echo "--- Restic repository not initialized. Running 'restic init' ---"
    restic init
}

echo "--- Starting Restic Backup Procedure ---"
LXC_IDS=$(pct list | awk 'NR>1 && $2=="running" {print $1}')

cleanup() {
    echo "--- Resuming Paused Containers ---"
    for VMID in $LXC_IDS; do
        if pct exec "$VMID" -- command -v docker >/dev/null 2>&1; then
            # We filter specifically for paused containers to safely resume them
            PAUSE_LIST=$(pct exec "$VMID" -- docker ps -q --filter "status=paused" --filter "label=com.homelab.backup.pause=true")
            for DC in $PAUSE_LIST; do
                pct exec "$VMID" -- docker unpause "$DC" > /dev/null
            done
        fi
    done
}

# Ensure containers are always unpaused, even if the script crashes, is cancelled, or fails
trap cleanup EXIT

for VMID in $LXC_IDS; do
    if pct exec "$VMID" -- command -v docker >/dev/null 2>&1; then
        PAUSE_LIST=$(pct exec "$VMID" -- docker ps -q --filter "label=com.homelab.backup.pause=true")
        for DC in $PAUSE_LIST; do
            pct exec "$VMID" -- docker pause "$DC" > /dev/null
        done
    fi
done

ensure_restic_repo

restic backup "${APPDATA_DIR}" --cleanup-cache

# Apply retention policy: Keep last 7 days, 4 weeks, and 3 months.
# Because Restic uses extreme block-level deduplication, this history
# will comfortably fit alongside your data on the 2TB drive without filling it up.
echo "--- Cleaning up old snapshots ---"
restic forget --keep-daily 7 --keep-weekly 4 --keep-monthly 3 --prune

echo "--- Backup Procedure Finalized Successfully ---"
