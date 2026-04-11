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
trap 'echo ""; ui_info "Cancelled."; exit 0' INT

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

ui_section "Create a New Stack"

if [[ -z "$STACK_NAME" ]]; then
    STACK_NAME=$(ui_input_required "Name of the new stack (LXC container)" "my-stack  •  Esc to cancel") || { ui_info "Cancelled."; exit 0; }
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

if [[ -z "$USE_DOCKER" && -z "$USE_WATCHTOWER" && -z "$USE_PROMTAIL" ]]; then
    USE_DOCKER="n"; USE_WATCHTOWER="n"; USE_PROMTAIL="n"

    # Step 1: Docker
    ui_confirm "Include Docker Compose?" "true" && USE_DOCKER="y" || USE_DOCKER="n"

    # Step 2: Watchtower + Promtail — only if Docker is enabled
    if [[ "$USE_DOCKER" == "y" ]]; then
        declare -a _selected
        mapfile -t _selected < <(ui_multiselect \
            --header "Select additional Docker components:" \
            --selected "Watchtower — automatic image updates,Promtail — log shipping to Loki" \
            "Watchtower — automatic image updates" \
            "Promtail — log shipping to Loki") || { ui_info "Cancelled."; exit 0; }
        for _item in "${_selected[@]:-}"; do
            [[ "$_item" == "Watchtower"* ]] && USE_WATCHTOWER="y"
            [[ "$_item" == "Promtail"* ]]   && USE_PROMTAIL="y"
        done
        unset _selected _item
    fi
else
    # Partial CLI flags supplied — fill in any missing ones interactively
    if [[ -z "$USE_DOCKER" ]]; then
        ui_confirm "Will this stack use Docker?" "true" && USE_DOCKER="y" || USE_DOCKER="n"
    fi
    if [[ "$USE_DOCKER" =~ ^[Yy]$ ]]; then
        [[ -z "$USE_WATCHTOWER" ]] && { ui_confirm "Include Watchtower for automatic updates?" "true" && USE_WATCHTOWER="y" || USE_WATCHTOWER="n"; }
        [[ -z "$USE_PROMTAIL" ]]   && { ui_confirm "Include Promtail for centralized logging to Loki?" && USE_PROMTAIL="y" || USE_PROMTAIL="n"; }
    else
        USE_WATCHTOWER="n"; USE_PROMTAIL="n"
    fi
fi

echo ""
ui_step "Creating infrastructure template for stack '${STACK_NAME}'..."
mkdir -p "${STACK_DIR}"
CREATED_STACK_DIR="${STACK_DIR}" # Mark directory for potential rollback

APPS=()

while true; do
    echo ""
    APP_NAME=$(ui_input "App name (leave empty to finish)" "") || break
    APP_NAME=$(ui_trim "$APP_NAME")
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
