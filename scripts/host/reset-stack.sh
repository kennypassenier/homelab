#!/usr/bin/env bash
# Script Name: reset-stack.sh
# Description: Safely resets a stack by wiping Docker state and all host app data for the LXC, but retains the LXC container.
# Usage: ./reset-stack.sh [OPTIONS] <VMID>

set -euo pipefail

FORCE_YES=""

function show_help() {
    echo "Usage: $0 [OPTIONS] <VMID>"
    echo "Options:"
    echo "  -y    Force reset without interactive confirmation"
    echo "  -h    Show this help message"
}

while getopts "yh" opt; do
    case ${opt} in
        y ) FORCE_YES="yes" ;;
        h ) show_help; exit 0 ;;
        \? ) show_help; exit 1 ;;
    esac
done
shift $((OPTIND -1))


if [[ $# -ne 1 ]]; then
    show_help
    exit 1
fi

VMID="$1"

echo "--- Stack Reset Utility ---"
echo "Target VMID: ${VMID}"
echo ""
echo "⚠️  WARNING: This will permanently delete all application data and Docker state for this container!"
echo "The LXC container itself (and its IP/configuration) will be kept intact."
echo ""

if [[ "$FORCE_YES" != "yes" ]]; then
    read -r -p "Are you sure you want to RESET? (Type 'yes' to confirm): " CONFIRM
    if [[ "$CONFIRM" != "yes" ]]; then
        echo "Aborted."
        exit 0
    fi
fi

echo "Wiping host application data at ${HOST_STORAGE_PATH}..."

echo "Stopping Docker containers in VM ${VMID}..."
# Stop all docker containers, remove them, and prune volumes safely
if pct exec "${VMID}" -- command -v docker >/dev/null 2>&1; then
    pct exec "${VMID}" -- bash -c 'docker stop $(docker ps -a -q) 2>/dev/null || true'
    pct exec "${VMID}" -- bash -c 'docker rm $(docker ps -a -q) 2>/dev/null || true'
    pct exec "${VMID}" -- bash -c 'docker volume rm $(docker volume ls -q) 2>/dev/null || true'
fi

echo "Wiping internal LXC GitOps directory..."
pct exec "${VMID}" -- rm -rf /opt/gitops

echo "Wiping all host application data in /opt/appdata/*..."
rm -rf /opt/appdata/*

echo "All application data for VMID ${VMID} has been successfully reset."
echo "You can now trigger 'node-sync.sh' inside the container to start fresh from your Git configuration."
