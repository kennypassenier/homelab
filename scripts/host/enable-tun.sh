#!/usr/bin/env bash
# Script Name: enable-tun.sh
# Description: Enables /dev/net/tun passthrough for an unprivileged LXC container.
#              Required for containers running a VPN client (e.g. gluetun) inside Docker.
#              Auto-detects whether TUN is needed by inspecting the stack's compose files —
#              no manual knowledge required. Safe to run on any LXC: exits cleanly if TUN
#              is not needed.
# Usage: ./enable-tun.sh [-h] <VMID>

set -euo pipefail

show_help() {
    echo "Usage: $0 [-h] <VMID>"
    echo "  -h    Show this help message"
    echo "Example: $0 105"
    echo ""
    echo "The script auto-detects the stack name from the LXC's GitOps cron job and checks"
    echo "whether any compose file uses /dev/net/tun. If not needed, it exits without making"
    echo "any changes. If needed, it configures passthrough and prompts for an LXC restart."
    exit 0
}

while getopts "h" opt; do
    case "$opt" in
        h) show_help ;;
        *) show_help ;;
    esac
done
shift $((OPTIND-1))

if [[ $# -ne 1 ]]; then
    show_help
fi

VMID="$1"
CONF_FILE="/etc/pve/lxc/${VMID}.conf"

if [[ ! -f "$CONF_FILE" ]]; then
    echo "Error: LXC config file not found for VMID ${VMID} at ${CONF_FILE}."
    exit 1
fi

# Auto-detect the stack name from the GitOps cron job inside the LXC.
# The cron line looks like: */5 * * * * root /opt/gitops/scripts/container/node-sync.sh <STACK>
STACK_NAME=$(pct exec "${VMID}" -- bash -c "grep -o 'node-sync.sh [^ ]*' /etc/cron.d/gitops-sync 2>/dev/null | awk '{print \$2}'" || true)

if [[ -z "$STACK_NAME" ]]; then
    echo "Error: Could not detect stack name from LXC ${VMID}."
    echo "Is this LXC bootstrapped? Expected cron job in /etc/cron.d/gitops-sync."
    exit 1
fi

echo "Detected stack: ${STACK_NAME}"

# Check whether any compose file in this stack references /dev/net/tun.
# If not, this LXC does not need TUN passthrough — exit cleanly without touching anything.
if ! grep -rl "/dev/net/tun" "stacks/${STACK_NAME}/" 2>/dev/null | grep -q .; then
    echo "Stack '${STACK_NAME}' does not use /dev/net/tun. No changes needed."
    exit 0
fi

# Idempotency check — prevent duplicate entries on re-runs
if grep -q "lxc.cgroup2.devices.allow: c 10:200" "${CONF_FILE}"; then
    echo "TUN passthrough is already configured for LXC ${VMID}. Nothing to do."
    exit 0
fi

# Ensure the host TUN device exists before adding it to the LXC config
if [[ ! -e "/dev/net/tun" ]]; then
    echo "Error: /dev/net/tun does not exist on this Proxmox host."
    echo "Enable the TUN module: modprobe tun"
    exit 1
fi

echo "Stack '${STACK_NAME}' uses /dev/net/tun — configuring passthrough for LXC ${VMID}..."

# 10:200 is the major:minor number for /dev/net/tun on Linux
cat <<LXCEOF >> "${CONF_FILE}"

# --- Added by enable-tun.sh (auto-detected for stack: ${STACK_NAME}) ---
# Allow the container's cgroup to access the TUN character device (major 10, minor 200).
# Required for VPN clients (e.g. gluetun) running inside Docker.
lxc.cgroup2.devices.allow: c 10:200 rwm

# Bind mount the host's TUN device node into the container so Docker can see it.
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
# ----------------------------------------------------------------------
LXCEOF

echo "Success! TUN passthrough configured for LXC ${VMID} (stack: ${STACK_NAME})."
echo ""
echo "Restart the LXC to apply:"
echo "  pct stop ${VMID} && pct start ${VMID}"
