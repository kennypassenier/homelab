#!/usr/bin/env bash
# Script Name: clean-stack.sh
# Description: Safely resets or completely destroys a stack and its associated data on the Proxmox host.
# Usage: ./clean-stack.sh <VMID> <STACK_NAME>

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <VMID> <STACK_NAME>"
    exit 1
fi

VMID="$1"
STACK_NAME="$2"
HOST_STORAGE_PATH="/opt/appdata/${STACK_NAME}"

echo "--- Stack Cleanup Utility ---"
echo "Target VMID: ${VMID}"
echo "Target Stack: ${STACK_NAME}"
echo ""
echo "Choose an action:"
echo "  [1] RESET   - Keep the LXC container (retains static IP & OS), but wipe all Docker containers, volumes, and app data."
echo "  [2] DESTROY - Completely delete the LXC container and all associated host data."
echo "  [3] CANCEL  - Exit without doing anything."
read -r -p "Enter your choice (1/2/3): " ACTION

case "$ACTION" in
    1)
        echo ""
        echo "⚠️  WARNING: This will permanently delete all application data and Docker state for '${STACK_NAME}'!"
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
        echo "You can now push new changes to Git and trigger 'node-sync.sh' inside the container to start fresh."
        ;;
    2)
        echo ""
        echo "⚠️  WARNING: This will completely DESTROY LXC ${VMID} and permanently delete all host data for '${STACK_NAME}'!"
        read -r -p "Are you sure you want to DESTROY? (Type 'yes' to confirm): " CONFIRM
        if [[ "$CONFIRM" != "yes" ]]; then
            echo "Aborted."
            exit 0
        fi

        echo "Stopping LXC container ${VMID}..."
        pct stop "${VMID}" || true

        echo "Destroying LXC container ${VMID}..."
        pct destroy "${VMID}"

        echo "Wiping host application data at ${HOST_STORAGE_PATH}..."
        rm -rf "${HOST_STORAGE_PATH:?}"

        echo "Stack '${STACK_NAME}' (VM ${VMID}) has been completely destroyed and cleaned up."
        ;;
    *)
        echo "Aborted."
        exit 0
        ;;
esac
