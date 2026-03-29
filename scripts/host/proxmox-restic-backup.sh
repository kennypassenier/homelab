#!/usr/bin/env bash
# Script Name: proxmox-restic-backup.sh
# Description: Centralized Restic backup for the host drive. Pauses containers by label.

set -euo pipefail

RESTIC_PASSWORD="jouw_restic_wachtwoord"
RESTIC_REPOSITORY="/pad/naar/nas/of/s3"
APPDATA_DIR="/HDD2TB/appdata"
export RESTIC_PASSWORD RESTIC_REPOSITORY

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

restic backup "${APPDATA_DIR}" --cleanup-cache

# Apply retention policy: Keep last 7 days, 4 weeks, and 3 months.
# Because Restic uses extreme block-level deduplication, this history
# will comfortably fit alongside your data on the 2TB drive without filling it up.
echo "--- Cleaning up old snapshots ---"
restic forget --keep-daily 7 --keep-weekly 4 --keep-monthly 3 --prune

echo "--- Backup Procedure Finalized Successfully ---"
