#!/usr/bin/env bash
# Script Name: register-local-node.sh
# Description: Dynamically updates the local SSH configuration on Pop!_OS after OPNsense setup.

set -euo pipefail

CONFIG_FILE="${HOME}/.ssh/config"
mkdir -p "${HOME}/.ssh"
touch "${CONFIG_FILE}"
chmod 600 "${CONFIG_FILE}"

echo "--- Local Workstation Network Configuration Updater ---"
read -r -p "Enter the logical Host alias (e.g., media-stack): " SSH_ALIAS
read -r -p "Enter the static IPv4 address (assigned via OPNsense Kea): " SSH_IP

if grep -qE "^Host\s+${SSH_ALIAS}$" "${CONFIG_FILE}"; then
    echo "Alias '${SSH_ALIAS}' is already defined in ${CONFIG_FILE}. Skipping."
    exit 1
fi

cat <<EOF >> "${CONFIG_FILE}"

Host ${SSH_ALIAS}
    HostName ${SSH_IP}
    User root
    Port 22
    IdentityFile ~/.ssh/id_ed25519
    StrictHostKeyChecking accept-new
EOF

echo "Done! You can now securely connect via: ssh ${SSH_ALIAS}"
