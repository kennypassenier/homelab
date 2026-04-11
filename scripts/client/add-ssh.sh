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

# 2. Find available stacks in stacks/ directory
AVAILABLE_STACKS=()
if [[ -d "stacks" ]]; then
    for dir in stacks/*/; do
        if [[ -d "$dir" ]]; then
            AVAILABLE_STACKS+=("$(basename "$dir")")
        fi
    done
fi

# 3. Interactive Menu
ui_section "Local Workstation SSH Configurator"

# Build display items and a reverse map from label -> stack name
declare -a MENU_ITEMS
declare -A ITEM_STACK

for stack in "${AVAILABLE_STACKS[@]}"; do
    if [[ -n "${EXISTING_HOSTS_IP[$stack]:-}" ]]; then
        current_ip="${EXISTING_HOSTS_IP[$stack]}"
        label="Update: ${stack}  (IP: ${current_ip})"
    else
        label="Create: ${stack}"
    fi
    MENU_ITEMS+=("$label")
    ITEM_STACK["$label"]="$stack"
done

MENU_ITEMS+=("Manually add a custom alias")
MENU_ITEMS+=("Exit")

CHOICE=$(ui_choose --header "Select an SSH alias to configure:" "${MENU_ITEMS[@]}") || {
    ui_info "Cancelled."
    SUCCESS=1
    exit 0
}

SSH_ALIAS=""
SSH_IP=""

if [[ "$CHOICE" == "Exit" ]]; then
    ui_info "Exited."
    SUCCESS=1
    exit 0
elif [[ "$CHOICE" == "Manually add a custom alias" ]]; then
    SSH_ALIAS=$(ui_input_required "New logical Host alias" "gateway") || { ui_info "Cancelled."; SUCCESS=1; exit 0; }
    SSH_IP=$(ui_input_required "Static IPv4 address" "10.10.10.x") || { ui_info "Cancelled."; SUCCESS=1; exit 0; }
elif [[ "$CHOICE" == Update:* ]]; then
    stack="${ITEM_STACK[$CHOICE]}"
    current_ip="${EXISTING_HOSTS_IP[$stack]}"
    SSH_ALIAS="$stack"
    SSH_IP=$(ui_input_required "New IPv4 for '${SSH_ALIAS}'" "10.10.10.x" "$current_ip") || { ui_info "Cancelled."; SUCCESS=1; exit 0; }
elif [[ "$CHOICE" == Create:* ]]; then
    SSH_ALIAS="${ITEM_STACK[$CHOICE]}"
    SSH_IP=$(ui_input_required "Static IPv4 for '${SSH_ALIAS}'" "10.10.10.x") || { ui_info "Cancelled."; SUCCESS=1; exit 0; }
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
