#!/usr/bin/env bash
# Script Name: add-ssh.sh
# Description: Interactive and idempotent script to manage SSH aliases in ~/.ssh/config.

set -euo pipefail

# Source the shared UI library
source "scripts/shared/lib-ui.sh"

CONFIG_FILE="${HOME}/.ssh/config"
TMP_CONFIG="${CONFIG_FILE}.tmp"
SUCCESS=0

# --- Rollback & Error Handling ---
cleanup_on_error() {
    local exit_code=$?
    # Only trigger rollback on actual errors before completion
    if [[ $exit_code -ne 0 && $SUCCESS -eq 0 ]]; then
        echo ""
        ui_error "SSH configuration failed unexpectedly! (Exit code: $exit_code)"

        # Rollback: Clean up temporary config files if awk failed midway
        if [[ -f "$TMP_CONFIG" ]]; then
            ui_warning "Initiating safety rollback..."
            ui_info "Removing temporary SSH configuration file to prevent corruption."
            rm -f "$TMP_CONFIG"
            ui_success "Rollback complete. ~/.ssh/config is untouched."
        fi

        echo ""
        ui_step "Troubleshooting tips:"
        ui_info "1. Ensure you have write permissions for ~/.ssh/config."
        ui_info "2. Verify the integrity of your current ~/.ssh/config file."
        echo ""
    fi
}
trap cleanup_on_error EXIT

# Ensure SSH config directory and file exist
mkdir -p "${HOME}/.ssh"
touch "${CONFIG_FILE}"
chmod 600 "${CONFIG_FILE}"

# 1. Parse existing hosts and IPs from ~/.ssh/config
declare -A EXISTING_HOSTS_IP

while IFS=':' read -r host ip; do
    if [[ "$host" != "*" && -n "$host" ]]; then
        EXISTING_HOSTS_IP["$host"]="$ip"
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

# 2. Find available stacks in apps/ directory
AVAILABLE_STACKS=()
if [[ -d "apps" ]]; then
    for dir in apps/*/; do
        if [[ -d "$dir" ]]; then
            AVAILABLE_STACKS+=("$(basename "$dir")")
        fi
    done
fi

# 3. Interactive Menu
ui_info "=== Local Workstation SSH Configurator ==="
echo "Select an SSH alias to configure:"
echo ""

index=1
declare -a MENU_ACTIONS

for stack in "${AVAILABLE_STACKS[@]}"; do
    if [[ -n "${EXISTING_HOSTS_IP[$stack]:-}" ]]; then
        current_ip="${EXISTING_HOSTS_IP[$stack]}"
        echo "  $index) Update: $stack (Current IP: $current_ip)"
        MENU_ACTIONS[$index]="UPDATE:$stack:$current_ip"
    else
        echo "  $index) Create: $stack"
        MENU_ACTIONS[$index]="CREATE:$stack"
    fi
    ((index++))
done

echo "  $index) Manually add a custom alias"
MENU_ACTIONS[$index]="MANUAL"
((index++))

echo "  $index) Exit"
MENU_ACTIONS[$index]="EXIT"
max_choice=$((index - 1))

echo ""
read -r -p "Select an option (1-$max_choice): " choice

if ! [[ "$choice" =~ ^[0-9]+$ ]] || [ "$choice" -lt 1 ] || [ "$choice" -gt "$max_choice" ]; then
    ui_error "Invalid selection."
    exit 1
fi

action="${MENU_ACTIONS[$choice]}"
SSH_ALIAS=""
SSH_IP=""

if [[ "$action" == "EXIT" ]]; then
    ui_info "Exited."
    SUCCESS=1
    exit 0
elif [[ "$action" == "MANUAL" ]]; then
    read -r -p "Enter the new logical Host alias (e.g., gateway): " SSH_ALIAS
    read -r -p "Enter the static IPv4 address (e.g., 10.10.10.6): " SSH_IP
elif [[ "$action" == UPDATE:* ]]; then
    IFS=':' read -r _ SSH_ALIAS current_ip <<< "$action"
    read -r -p "Enter the new static IPv4 address for '$SSH_ALIAS' [current: $current_ip]: " new_ip
    SSH_IP="${new_ip:-$current_ip}"
elif [[ "$action" == CREATE:* ]]; then
    IFS=':' read -r _ SSH_ALIAS <<< "$action"
    read -r -p "Enter the static IPv4 address for stack '$SSH_ALIAS': " SSH_IP
fi

if [[ -z "$SSH_ALIAS" || -z "$SSH_IP" ]]; then
    ui_error "Alias and IP cannot be empty."
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
    ui_success "Alias '${SSH_ALIAS}' is already correctly configured for ${SSH_IP}. Skipping."
    SUCCESS=1
    exit 0
fi

if grep -iq "^Host[[:space:]]\+${SSH_ALIAS}$" "$CONFIG_FILE"; then
    ui_step "Updating existing alias '${SSH_ALIAS}'..."
    awk -v target="$SSH_ALIAS" '
    tolower($1) == "host" || tolower($1) == "match" {
        if (tolower($1) == "host" && $2 == target) skip = 1
        else skip = 0
    }
    !skip { print }
    ' "$CONFIG_FILE" > "$TMP_CONFIG"

    cat "$TMP_CONFIG" > "$CONFIG_FILE"
    rm -f "$TMP_CONFIG"
else
    ui_step "Adding new alias '${SSH_ALIAS}'..."
fi

cat <<EOF >> "${CONFIG_FILE}"

Host ${SSH_ALIAS}
    HostName ${SSH_IP}
    User root
    Port 22
    StrictHostKeyChecking accept-new
EOF

# Mark execution as completely successful to prevent rollback
SUCCESS=1

echo ""
ui_success "Done! You can now securely connect via: ssh ${SSH_ALIAS}"
