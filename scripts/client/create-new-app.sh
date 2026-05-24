# --- Auto-create all /appdata bind-mount directories from docker-compose.yml (for new app) ---
COMPOSE_FILE="stacks/${STACK_NAME}/${APP_NAME}/docker-compose.yml"
if [ -f "$COMPOSE_FILE" ]; then
    grep '^[[:space:]]*-[[:space:]]*/appdata' "$COMPOSE_FILE" | cut -d: -f1 | sed 's/^[[:space:]]*-[[:space:]]*//' | while read DIR; do
        if [ ! -d "$DIR" ]; then
            mkdir -p "$DIR"
            ui_info "[create-new-app] Aangemaakt: $DIR"
        fi
    done
fi
#!/usr/bin/env bash
# Script Name: create-new-app.sh
# Description: Generates a new application template within an existing stack using shared library functions.

set -euo pipefail

# Source the shared libraries
source "$(dirname "$0")/lib/lib-stack.sh"
source "scripts/shared/lib-ui.sh"

# Ensure we are running from the root of the repo
require_repo_root

ui_section "Add a New App to an Existing Stack"

# Select an existing stack using the library function
# prompt_stack_selection returns 2 if the user chose Cancel
STACK_NAME=$(prompt_stack_selection) || { ui_info "Cancelled."; exit 0; }

if [[ -z "$STACK_NAME" ]]; then
    ui_error "No stack selected or available."
    exit 1
fi

ui_step "Selected stack: ${STACK_NAME}"
echo ""

# Prompt for the new app name
while true; do
    APP_NAME=$(ui_input_required "Enter the new app name" "my-app  •  Esc to cancel") || { ui_info "Cancelled."; exit 0; }
    if [[ -n "$APP_NAME" ]]; then
        # Check if the directory already exists
        if [[ -d "stacks/${STACK_NAME}/${APP_NAME}" ]]; then
            ui_error "App '${APP_NAME}' already exists in stack '${STACK_NAME}'. Please choose a different name."
        else
            break
        fi
    else
        ui_error "App name cannot be empty."
    fi
done

# Prompt for Docker usage
if ui_confirm "Will this app use Docker?" "true"; then
    USE_DOCKER="y"
else
    USE_DOCKER="n"
fi

# Generate the app using the shared function
ui_step "Creating infrastructure template for app '${APP_NAME}' in stack '${STACK_NAME}'..."

# Call the generator function
generate_app "${STACK_NAME}" "${APP_NAME}" "${USE_DOCKER}"

# --- Infisical .env export automation ---
PRE_SYNC="stacks/${STACK_NAME}/pre-sync.sh"
EXPORT_LINE=""

# Determine the correct infisical export command
if [[ "${APP_NAME}" == "watchtower" ]]; then
    EXPORT_LINE="" # Do not add for watchtower
elif [[ "${APP_NAME}" == "promtail" ]]; then
    EXPORT_LINE="infisical export --env=prod --path=shared/promtail/.env > /appdata/${STACK_NAME}/promtail/.env"
else
    EXPORT_LINE="infisical export --env=prod --path=${STACK_NAME}/${APP_NAME}/.env > /appdata/${STACK_NAME}/${APP_NAME}/.env"
fi

# Append to pre-sync.sh if not already present and line is not empty
if [[ -n "$EXPORT_LINE" ]]; then
    if ! grep -Fxq "$EXPORT_LINE" "$PRE_SYNC" 2>/dev/null; then
        echo "$EXPORT_LINE" >> "$PRE_SYNC"
        ui_success "Added Infisical export for ${APP_NAME} to pre-sync.sh."
    else
        ui_info "Infisical export for ${APP_NAME} already present in pre-sync.sh."
    fi
fi

echo ""
ui_success "App generation completed."
ui_info "You can now edit the docker-compose.yml and .env files directly."
ui_info "Infisical .env export for this app is now automated in pre-sync.sh."
