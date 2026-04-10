#!/usr/bin/env bash
# Script Name: create-new-stack.sh
# Description: Generates a new application stack template using shared library functions.

set -euo pipefail

# Source the shared libraries
source "$(dirname "$0")/lib/lib-stack.sh"
source "scripts/shared/lib-ui.sh"

# Global variables for rollback tracking
CREATED_STACK_DIR=""
SUCCESS=0

# --- Rollback & Error Handling ---
cleanup_on_error() {
    local exit_code=$?
    # Only trigger rollback on actual errors before completion
    if [[ $exit_code -ne 0 && $SUCCESS -eq 0 ]]; then
        echo ""
        ui_error "Stack generation failed unexpectedly! (Exit code: $exit_code)"

        # Rollback: Remove the partially created stack directory
        if [[ -n "$CREATED_STACK_DIR" && -d "$CREATED_STACK_DIR" ]]; then
            ui_warning "Initiating safety rollback..."
            ui_info "Removing partially created stack directory '${CREATED_STACK_DIR}' to prevent artifacts."
            rm -rf "$CREATED_STACK_DIR"
            ui_success "Rollback complete. System is clean."
        fi

        echo ""
        ui_step "Troubleshooting tips:"
        ui_info "1. Check your write permissions in the 'stacks/' directory."
        ui_info "2. Ensure you are running this script from the root of the repository."
        ui_info "3. Verify your disk is not full."
        echo ""
    fi
}
trap cleanup_on_error EXIT

USE_DOCKER=""
USE_PROMTAIL=""
USE_WATCHTOWER=""

function show_help() {
    echo "Usage: $0 [OPTIONS] [STACK_NAME]"
    echo "Options:"
    echo "  -d    Force use Docker without prompting"
    echo "  -w    Include centralized Watchtower (requires Docker)"
    echo "  -p    Include centralized Promtail for Loki (requires Docker)"
    echo "  -h    Show this help message"
}

while getopts "dwph" opt; do
    case ${opt} in
        d ) USE_DOCKER="y" ;;
        w ) USE_WATCHTOWER="y" ;;
        p ) USE_PROMTAIL="y" ;;
        h ) show_help; exit 0 ;;
        \? ) show_help; exit 1 ;;
    esac
done
shift $((OPTIND -1))

STACK_NAME="${1:-}"

# Ensure we are running from the root of the repo
require_repo_root

ui_info "=== Create a New Stack ==="

if [[ -z "$STACK_NAME" ]]; then
    read -r -p "Enter the name of the new stack (LXC container): " STACK_NAME
fi

if [[ -z "$STACK_NAME" ]]; then
    ui_error "Stack name cannot be empty."
    exit 1
fi

STACK_DIR="stacks/${STACK_NAME}"

# Prevent overwriting an existing stack
if [[ -d "$STACK_DIR" ]]; then
    ui_error "Stack '${STACK_NAME}' already exists. Use 'create-new-app.sh' to add stacks to it."
    exit 1
fi

if [[ -z "$USE_DOCKER" ]]; then
    read -r -p "Will this stack use Docker? (y/n) [y]: " USE_DOCKER
    USE_DOCKER=${USE_DOCKER:-y}
fi

if [[ "$USE_DOCKER" =~ ^[Yy]$ ]]; then
    if [[ -z "$USE_WATCHTOWER" ]]; then
        read -r -p "Include Watchtower for automatic updates? (y/n) [y]: " USE_WATCHTOWER
        USE_WATCHTOWER=${USE_WATCHTOWER:-y}
    fi
    if [[ -z "$USE_PROMTAIL" ]]; then
        read -r -p "Include Promtail for centralized logging to Loki? (y/n) [n]: " USE_PROMTAIL
        USE_PROMTAIL=${USE_PROMTAIL:-n}
    fi
else
    USE_WATCHTOWER="n"
    USE_PROMTAIL="n"
fi

echo ""
ui_step "Creating infrastructure template for stack '${STACK_NAME}'..."
mkdir -p "${STACK_DIR}"
CREATED_STACK_DIR="${STACK_DIR}" # Mark directory for potential rollback

APPS=()

while true; do
    echo ""
    read -r -p "Enter app name for this stack (leave empty to finish): " APP_NAME
    if [[ -z "$APP_NAME" ]]; then
        break
    fi

    if [[ -d "${STACK_DIR}/${APP_NAME}" ]]; then
        ui_warning "App '${APP_NAME}' already exists in this stack. Please choose another name."
        continue
    fi

    generate_app "${STACK_NAME}" "${APP_NAME}" "${USE_DOCKER}"
    APPS+=("${APP_NAME}")
    ui_success "App '${APP_NAME}' template generated."
done

# Generate central Watchtower for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_WATCHTOWER" =~ ^[Yy]$ ]] && [ ${#APPS[@]} -gt 0 ]; then
    generate_watchtower "${STACK_NAME}"
    ui_success "Central Watchtower configured."
fi

# Generate central Promtail for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_PROMTAIL" =~ ^[Yy]$ ]]; then
    generate_promtail "${STACK_NAME}"
    ui_success "Central Promtail configured. (LOKI_IP set via .env)"
fi

# Mark execution as completely successful to prevent rollback
SUCCESS=1

echo ""
ui_success "Stack generation completed successfully!"
ui_info "You can now edit the docker-compose.yml and .env files directly."
ui_info "When you run 'git add', Git and SOPS will invisibly encrypt the .env files for you."
