#!/usr/bin/env bash
# Script Name: host.sh
# Description: Central management menu for all host-side homelab operations.
# Executed on the Proxmox host.

set -euo pipefail

# Ensure we are running from the root of the repo
if [[ ! -d "stacks" || ! -d "scripts" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

source "scripts/shared/lib-ui.sh"

show_menu() {
    clear
    ui_header "Homelab Host Manager"
    echo -e "  ${C_GREEN}1.${C_NC} Bootstrap a new LXC container"
    echo -e "  ${C_GREEN}2.${C_NC} Backup Stacks (Restic)"
    echo -e "  ${C_GREEN}3.${C_NC} Enable GPU Passthrough for an LXC"
    echo -e "  ${C_GREEN}4.${C_NC} Reset a corrupted Stack"
    echo -e "  ${C_GREEN}5.${C_NC} Sync Host scripts from Git"
    echo -e "  ${C_GREEN}6.${C_NC} Setup Host Cronjob for automated sync"
    echo -e "  ${C_YELLOW}0.${C_NC} Exit"
    echo ""
}

while true; do
    show_menu
    read -r -p "${UI_INDENT}Select an option (0-6): " choice

    case $choice in
        1)
            echo ""
            ./scripts/host/bootstrap-lxc.sh
            ;;
        2)
            echo ""
            ./scripts/host/backup-stacks.sh
            ;;
        3)
            echo ""
            ./scripts/host/enable-gpu.sh
            ;;
        4)
            echo ""
            ./scripts/host/reset-stack.sh
            ;;
        5)
            echo ""
            ./scripts/host/sync-host.sh
            ;;
        6)
            echo ""
            ./scripts/host/setup-cron.sh
            ;;
        0)
            echo ""
            ui_info "Exiting Host Manager."
            exit 0
            ;;
        *)
            ui_error "Invalid selection. Please enter a number between 0 and 6."
            sleep 2
            ;;
    esac

    echo ""
    read -r -p "${UI_INDENT}Press Enter to return to the menu..."
done
