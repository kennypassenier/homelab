#!/usr/bin/env bash
# Script Name: proxmox-reset-stack.sh
# Description: Safely resets a stack by wiping Docker state and host app data, but retains the LXC container.
# Usage: ./proxmox-reset-stack.sh <VMID> <STACK_NAME>

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <VMID> <STACK_NAME>"
    exit 1
fi

VMID="$1"
STACK_NAME="$2"
HOST_STORAGE_PATH="/opt/appdata/${STACK_NAME}"

echo "--- Stack Reset Utility ---"
echo "Target VMID: ${VMID}"
echo "Target Stack: ${STACK_NAME}"
echo ""
echo "⚠️  WARNING: This will permanently delete all application data and Docker state for '${STACK_NAME}'!"
echo "The LXC container itself (and its IP/configuration) will be kept intact."
echo ""

read -r -p "Are you sure you want to RESET? (Type 'yes' to confirm): " CONFIRM
if [[ "$CONFIRM" != "yes" ]]; then
    echo "Aborted."
    exit 0
fi

echo "Stopping Docker containers in VM ${VMID}..."
# Stop all docker containers, remove them, and prune volumes safely
if pct exec "${VMID}" -- command -v docker >/dev/null 2>&1; then
    pct exec "${VMID}" -- bash -c 'docker stop $(docker ps -a -q) 2>/dev/null || true'
    pct exec "${VMID}" -- bash -c 'docker rm $(docker ps -a -q) 2>/dev/null || true'
    pct exec "${VMID}" -- bash -c 'docker volume rm $(docker volume ls -q) 2>/dev/null || true'
fi

echo "Wiping host application data at ${HOST_STORAGE_PATH}..."
# Delete contents but keep the directory and mount intact
# Using :? ensures it aborts if variable is empty, preventing accidental root wipe
rm -rf "${HOST_STORAGE_PATH:?}"/*

echo "Stack '${STACK_NAME}' has been successfully reset."
echo "You can now trigger 'node-sync.sh' inside the container to start fresh from your Git configuration."
