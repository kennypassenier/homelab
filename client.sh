#!/usr/bin/env bash
# Script Name: client.sh
# Description: Central management menu for all client-side homelab operations.
# Executed on the local workstation (e.g., Pop!_OS).

set -euo pipefail

# Ensure we are running from the root of the repo
if [[ ! -d "apps" || ! -d "scripts" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

source "scripts/shared/lib-ui.sh"

show_menu() {
    clear
    echo -e "${C_CYAN}================================================================${C_NC}"
    echo -e "${C_CYAN}                   Homelab Client Manager                       ${C_NC}"
    echo -e "${C_CYAN}================================================================${C_NC}"
    echo ""
    echo -e "  ${C_GREEN}1.${C_NC} Create a new Stack"
    echo -e "  ${C_GREEN}2.${C_NC} Create a new App inside a Stack"
    echo -e "  ${C_GREEN}3.${C_NC} Remove an App"
    echo -e "  ${C_GREEN}4.${C_NC} Remove an entire Stack"
    echo -e "  ${C_GREEN}5.${C_NC} Register SSH alias for a new LXC container"
    echo -e "  ${C_GREEN}6.${C_NC} Initialize Ground Zero (SOPS Encryption Setup)"
    echo -e "  ${C_YELLOW}0.${C_NC} Exit"
    echo ""
}

while true; do
    show_menu
    read -r -p "Select an option (0-6): " choice

    case $choice in
        1)
            echo ""
            ./scripts/client/create-new-stack.sh
            ;;
        2)
            echo ""
            ./scripts/client/create-new-app.sh
            ;;
        3)
            echo ""
            ./scripts/client/remove-app.sh
            ;;
        4)
            echo ""
            ./scripts/client/remove-stack.sh
            ;;
        5)
            echo ""
            ./scripts/client/add-ssh.sh
            ;;
        6)
            echo ""
            ./scripts/client/init-ground-zero.sh
            ;;
        0)
            echo ""
            ui_info "Exiting Client Manager."
            exit 0
            ;;
        *)
            ui_error "Invalid selection. Please enter a number between 0 and 6."
            sleep 2
            ;;
    esac

    echo ""
    read -r -p "Press Enter to return to the menu..."
done
