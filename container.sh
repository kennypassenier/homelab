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
            # Auto-detect the stack name from the GitOps cron job — the same stack
            # this container was bootstrapped for is always in /etc/cron.d/gitops-sync.
            STACK_NAME=$(grep -o 'node-sync.sh [^ ]*' /etc/cron.d/gitops-sync 2>/dev/null | awk '{print $2}' || true)
            if [[ -n "$STACK_NAME" ]]; then
                ui_info "Auto-detected stack: ${STACK_NAME}"
                ./scripts/container/node-sync.sh "$STACK_NAME"
            else
                ui_error "Could not auto-detect stack name from /etc/cron.d/gitops-sync."
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
