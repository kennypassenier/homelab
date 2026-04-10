#!/usr/bin/env bash
# Script Name: add-ssh.sh
# Description: Idempotent script to add or update an SSH alias in the local ~/.ssh/config.

set -euo pipefail

CONFIG_FILE="${HOME}/.ssh/config"

show_help() {
    echo "Usage: $0 [-a <alias>] [-i <ip_address>] [-h]"
    echo ""
    echo "Options:"
    echo "  -a    Logical SSH Host alias (e.g., media)"
    echo "  -i    Static IPv4 address of the LXC container"
    echo "  -h    Show this help message"
    exit 0
}

SSH_ALIAS=""
SSH_IP=""

while getopts "a:i:h" opt; do
    case "$opt" in
        a) SSH_ALIAS="$OPTARG" ;;
        i) SSH_IP="$OPTARG" ;;
        h) show_help ;;
        *) show_help ;;
    esac
done
shift $((OPTIND-1))

echo "--- Local Workstation SSH Configurator ---"

if [[ -z "$SSH_ALIAS" ]]; then
    read -r -p "Enter the logical Host alias (e.g., gateway): " SSH_ALIAS
fi

if [[ -z "$SSH_ALIAS" ]]; then
    echo "Error: Alias cannot be empty."
    exit 1
fi

if [[ -z "$SSH_IP" ]]; then
    read -r -p "Enter the static IPv4 address (e.g., 10.10.10.6): " SSH_IP
fi

if [[ -z "$SSH_IP" ]]; then
    echo "Error: IP address cannot be empty."
    exit 1
fi

# Ensure SSH config directory and file exist
mkdir -p "${HOME}/.ssh"
touch "${CONFIG_FILE}"
chmod 600 "${CONFIG_FILE}"

# Function to extract a specific property for a given Host block
get_ssh_property() {
    local host="$1"
    local prop="$2"
    awk -v h="$host" -v p="$prop" '
    tolower($1) == "host" || tolower($1) == "match" {
        if (tolower($1) == "host" && $2 == h) in_block = 1
        else in_block = 0
    }
    in_block && tolower($1) == tolower(p) {
        print $2
        exit
    }
    ' "$CONFIG_FILE"
}

CURRENT_IP=$(get_ssh_property "$SSH_ALIAS" "hostname")
CURRENT_USER=$(get_ssh_property "$SSH_ALIAS" "user")
CURRENT_PORT=$(get_ssh_property "$SSH_ALIAS" "port")
CURRENT_SHKC=$(get_ssh_property "$SSH_ALIAS" "stricthostkeychecking")

# Check if the configuration is already perfectly compliant
if [[ "$CURRENT_IP" == "$SSH_IP" ]] && \
   [[ "$CURRENT_USER" == "root" ]] && \
   [[ "${CURRENT_PORT:-22}" == "22" ]] && \
   [[ "$CURRENT_SHKC" == "accept-new" ]]; then
    echo "Alias '${SSH_ALIAS}' is already correctly configured for ${SSH_IP}. Skipping."
    exit 0
fi

# If the alias exists but is incorrect, we remove the old block safely
if grep -iq "^Host[[:space:]]\+${SSH_ALIAS}$" "$CONFIG_FILE"; then
    echo "Updating existing alias '${SSH_ALIAS}'..."
    # Awk logic: skip lines when we are inside the target Host block
    awk -v target="$SSH_ALIAS" '
    tolower($1) == "host" || tolower($1) == "match" {
        if (tolower($1) == "host" && $2 == target) {
            skip = 1
        } else {
            skip = 0
        }
    }
    !skip { print }
    ' "$CONFIG_FILE" > "${CONFIG_FILE}.tmp"

    # Preserve permissions and overwrite
    cat "${CONFIG_FILE}.tmp" > "$CONFIG_FILE"
    rm -f "${CONFIG_FILE}.tmp"
else
    echo "Adding new alias '${SSH_ALIAS}'..."
fi

# Append the standardized block at the end of the file
cat <<EOF >> "${CONFIG_FILE}"

Host ${SSH_ALIAS}
    HostName ${SSH_IP}
    User root
    Port 22
    StrictHostKeyChecking accept-new
EOF

echo "Done! You can now securely connect via: ssh ${SSH_ALIAS}"
