#!/usr/bin/env bash
# Script Name: add-ssh.sh
# Description: Interactive and idempotent script to manage SSH aliases in ~/.ssh/config.

set -euo pipefail

CONFIG_FILE="${HOME}/.ssh/config"

# Ensure SSH config directory and file exist
mkdir -p "${HOME}/.ssh"
touch "${CONFIG_FILE}"
chmod 600 "${CONFIG_FILE}"

# 1. Parse existing hosts and IPs from ~/.ssh/config
EXISTING_HOSTS=()
EXISTING_IPS=()

while IFS=':' read -r host ip; do
    if [[ "$host" != "*" && -n "$host" ]]; then
        EXISTING_HOSTS+=("$host")
        EXISTING_IPS+=("$ip")
    fi
done < <(awk '
tolower($1) == "host" {
    if (host != "") print host ":" ip
    host = $2
    ip = "unknown"
}
tolower($1) == "hostname" && host != "" {
    ip = $2
}
END {
    if (host != "") print host ":" ip
}' "$CONFIG_FILE")

# 2. Find suggestions based on apps/ directory
SUGGESTIONS=()
if [[ -d "apps" ]]; then
    for dir in apps/*/; do
        if [[ -d "$dir" ]]; then
            stack=$(basename "$dir")
            found=0
            for e_host in "${EXISTING_HOSTS[@]:-}"; do
                if [[ "$e_host" == "$stack" ]]; then
                    found=1
                    break
                fi
            done
            if [[ $found -eq 0 ]]; then
                SUGGESTIONS+=("$stack")
            fi
        fi
    done
fi

# 3. Interactive Menu
echo "=== Local Workstation SSH Configurator ==="
echo "Existing SSH Aliases in ~/.ssh/config:"

index=1
declare -a MENU_ACTIONS

if [[ ${#EXISTING_HOSTS[@]} -eq 0 ]]; then
    echo "  (No aliases found)"
else
    for i in "${!EXISTING_HOSTS[@]}"; do
        host="${EXISTING_HOSTS[$i]}"
        ip="${EXISTING_IPS[$i]}"
        echo "  $index) Update existing: $host ($ip)"
        MENU_ACTIONS[$index]="UPDATE:$host:$ip"
        ((index++))
    done
fi

echo ""
if [[ ${#SUGGESTIONS[@]} -gt 0 ]]; then
    echo "Suggested homelab stacks to add:"
    for stack in "${SUGGESTIONS[@]}"; do
        echo "  $index) Add suggestion: $stack"
        MENU_ACTIONS[$index]="SUGGEST:$stack"
        ((index++))
    done
    echo ""
fi

echo "  $index) Manually add a new custom alias"
MENU_ACTIONS[$index]="MANUAL"
((index++))

echo "  $index) Exit"
MENU_ACTIONS[$index]="EXIT"
max_choice=$((index - 1))

echo ""
read -r -p "Select an option (1-$max_choice): " choice

if ! [[ "$choice" =~ ^[0-9]+$ ]] || [ "$choice" -lt 1 ] || [ "$choice" -gt "$max_choice" ]; then
    echo "Invalid selection."
    exit 1
fi

action="${MENU_ACTIONS[$choice]}"
SSH_ALIAS=""
SSH_IP=""

if [[ "$action" == "EXIT" ]]; then
    echo "Exited."
    exit 0
elif [[ "$action" == "MANUAL" ]]; then
    read -r -p "Enter the new logical Host alias (e.g., gateway): " SSH_ALIAS
    read -r -p "Enter the static IPv4 address (e.g., 10.10.10.6): " SSH_IP
elif [[ "$action" == UPDATE:* ]]; then
    IFS=':' read -r _ SSH_ALIAS current_ip <<< "$action"
    read -r -p "Enter the new static IPv4 address for '$SSH_ALIAS' [current: $current_ip]: " new_ip
    SSH_IP="${new_ip:-$current_ip}"
elif [[ "$action" == SUGGEST:* ]]; then
    IFS=':' read -r _ SSH_ALIAS <<< "$action"
    read -r -p "Enter the static IPv4 address for suggested stack '$SSH_ALIAS': " SSH_IP
fi

if [[ -z "$SSH_ALIAS" || -z "$SSH_IP" ]]; then
    echo "Error: Alias and IP cannot be empty."
    exit 1
fi

# 4. Idempotency and Update Logic
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

if [[ "$CURRENT_IP" == "$SSH_IP" ]] && \
   [[ "$CURRENT_USER" == "root" ]] && \
   [[ "${CURRENT_PORT:-22}" == "22" ]] && \
   [[ "$CURRENT_SHKC" == "accept-new" ]]; then
    echo "Alias '${SSH_ALIAS}' is already correctly configured for ${SSH_IP}. Skipping."
    exit 0
fi

if grep -iq "^Host[[:space:]]\+${SSH_ALIAS}$" "$CONFIG_FILE"; then
    echo "Updating existing alias '${SSH_ALIAS}'..."
    awk -v target="$SSH_ALIAS" '
    tolower($1) == "host" || tolower($1) == "match" {
        if (tolower($1) == "host" && $2 == target) skip = 1
        else skip = 0
    }
    !skip { print }
    ' "$CONFIG_FILE" > "${CONFIG_FILE}.tmp"
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
