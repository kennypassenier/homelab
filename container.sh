#!/usr/bin/env bash
# Script Name: container.sh
# Description: Central management menu for all container-side homelab operations.
# Executed inside the Proxmox LXC container.

set -euo pipefail

# Ensure we are running from the root of the repo (usually /opt/gitops)
if [[ ! -d "stacks" || ! -d "scripts" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

source "scripts/shared/lib-ui.sh"

show_menu() {
    clear
    ui_header "Homelab Container Manager"
    echo -e "  ${C_GREEN}1.${C_NC} Trigger Node Sync (Pull from Git & Deploy)"
    echo -e "  ${C_YELLOW}0.${C_NC} Exit"
    echo ""
}

while true; do
    show_menu
    read -r -p "${UI_INDENT}Select an option (0-1): " choice

    case $choice in
        1)
            echo ""
            # node-sync.sh usually requires a stack name as an argument
            read -r -p "${UI_INDENT}Enter the stack name to sync: " STACK_NAME
            if [[ -n "$STACK_NAME" ]]; then
                ./scripts/container/node-sync.sh "$STACK_NAME"
            else
                ui_error "Stack name cannot be empty."
            fi
            ;;
        0)
            echo ""
            ui_info "Exiting Container Manager."
            exit 0
            ;;
        *)
            ui_error "Invalid selection. Please enter 0 or 1."
            sleep 2
            ;;
    esac

    echo ""
    read -r -p "${UI_INDENT}Press Enter to return to the menu..."
done
