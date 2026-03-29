#!/usr/bin/env bash
# Script Name: enable-gpu.sh
# Description: Enables Intel/AMD hardware GPU passthrough for an unprivileged LXC.
# Usage: ./enable-gpu.sh [-h] <VMID>

set -euo pipefail

show_help() {
    echo "Usage: $0 [-h] <VMID>"
    echo "  -h    Show this help message"
    echo "Example: $0 106"
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

# Check if passthrough is already enabled to prevent duplicate entries
if grep -q "lxc.cgroup2.devices.allow: c 226:" "$CONF_FILE"; then
    echo "GPU passthrough is already configured in ${CONF_FILE}."
    echo "If it is not working, consider removing the old lines manually and re-running this script."
    exit 0
fi

echo "Configuring GPU passthrough for LXC ${VMID}..."

# We append the cgroup and mount entries needed for /dev/dri
# Note: 226 is the major node number for DRM (Direct Rendering Manager) devices in Linux
cat <<EOF >> "$CONF_FILE"

# --- Added by proxmox-enable-gpu-passthrough.sh ---
# Allow container cgroups to access GPU devices (card0 and renderD*)
lxc.cgroup2.devices.allow: c 226:0 rwm
lxc.cgroup2.devices.allow: c 226:128 rwm

# Bind mount the host's GPU nodes into the container
lxc.mount.entry: /dev/dri/card0 dev/dri/card0 none bind,optional,create=file
lxc.mount.entry: /dev/dri/renderD128 dev/dri/renderD128 none bind,optional,create=file
# --------------------------------------------------
EOF

echo "Success! GPU passthrough settings appended to ${CONF_FILE}."
echo "Please restart LXC ${VMID} to apply the changes:"
echo "  pct stop ${VMID} && pct start ${VMID}"
