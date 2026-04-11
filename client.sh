#!/usr/bin/env bash
# Script Name: client.sh
# Description: Central management menu for all client-side homelab operations.
# Executed on the local workstation (e.g., Linux desktop).

set -euo pipefail

# Ensure we are running from the root of the repo
if [[ ! -d "stacks" || ! -d "scripts" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

source "scripts/shared/lib-ui.sh"

while true; do
    clear
    ui_header "Homelab Client Manager"

    CHOICE=$(ui_choose --header "Select an operation:" \
        "Create a new Stack" \
        "Create a new App inside a Stack" \
        "Remove an App" \
        "Remove an entire Stack" \
        "Register SSH alias for a new LXC" \
        "SOPS/Age: First-Time Key Setup (run once!)" \
        "SOPS/Age: Restore on New Machine" \
        "Exit") || CHOICE="Exit"

    clear

    case "$CHOICE" in
        "Create a new Stack")
            ./scripts/client/create-new-stack.sh ;;
        "Create a new App inside a Stack")
            ./scripts/client/create-new-app.sh ;;
        "Remove an App")
            ./scripts/client/remove-app.sh ;;
        "Remove an entire Stack")
            ./scripts/client/remove-stack.sh ;;
        "Register SSH alias for a new LXC")
            ./scripts/client/add-ssh.sh ;;
        "SOPS/Age: First-Time Key Setup (run once!)")
            if [[ -f "secrets/age.key.enc" ]]; then
                ui_warning "secrets/age.key.enc already exists — this setup has already been run."
                ui_warning "Running this again will OVERWRITE your existing encryption key."
                ui_warning "All existing encrypted .env files will become PERMANENTLY unreadable."
                echo ""
                if ! ui_confirm "Are you absolutely sure you want to generate a NEW key?" "false"; then
                    ui_info "Aborted."
                else
                    if ! ui_confirm "FINAL WARNING: this is irreversible. Continue?" "false"; then
                        ui_info "Aborted."
                    else
                        ./scripts/client/init-ground-zero.sh
                    fi
                fi
            else
                ./scripts/client/init-ground-zero.sh
            fi ;;
        "SOPS/Age: Restore on New Machine")
            ./scripts/client/restore-client.sh ;;
        "Exit")
            ui_info "Exiting Client Manager."
            exit 0
            ;;
    esac

    echo ""
    read -r -p "${UI_INDENT}Press Enter to return to the menu..."
done
