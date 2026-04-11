#!/usr/bin/env bash
# Script Name: remove-app.sh
# Description: Removes an app from a stack and commits the deletion to trigger Garbage Collection.

set -euo pipefail

# Source the shared libraries
source "$(dirname "$0")/lib/lib-stack.sh"
source "scripts/shared/lib-ui.sh"

# Ensure we are running from the root of the repo
require_repo_root

SUCCESS=0
APP_DIR=""

# --- Rollback & Error Handling ---
cleanup_on_error() {
    local exit_code=$?
    if [[ $exit_code -ne 0 && $SUCCESS -eq 0 ]]; then
        echo ""
        ui_error "App removal failed unexpectedly! (Exit code: $exit_code)"

        # Rollback: If we deleted the app directory from Git but failed to commit/push, restore it.
        if [[ -n "$APP_DIR" && ! -d "$APP_DIR" ]]; then
            ui_warning "Initiating safety rollback..."
            ui_info "Restoring deleted files from Git..."
            git restore --staged "$APP_DIR" 2>/dev/null || true
            git restore "$APP_DIR" 2>/dev/null || true
            ui_success "Rollback complete. App configuration restored."
        fi

        echo ""
        ui_step "Troubleshooting tips:"
        ui_info "1. Check your Git repository state (git status)."
        ui_info "2. Ensure you have network access to push to the remote."
        echo ""
    fi
}
trap cleanup_on_error EXIT

ui_section "Remove an App from an Existing Stack"

# Select an existing stack using the library function
# prompt_stack_selection returns 2 if the user chose Cancel
STACK_NAME=$(prompt_stack_selection) || { ui_info "Cancelled."; SUCCESS=1; exit 0; }

ui_step "Selected stack: ${STACK_NAME}"
echo ""

# Select an existing app using the library function
# prompt_app_selection returns 2 if the user chose Cancel
APP_NAME=$(prompt_app_selection "$STACK_NAME") || { ui_info "Cancelled."; SUCCESS=1; exit 0; }

APP_DIR="stacks/${STACK_NAME}/${APP_NAME}"

# --- Confirmation Summary ---
echo ""
ui_divider "$C_RED"
echo -e "${UI_INDENT}${C_RED}!! DESTRUCTIVE ACTION — THIS CANNOT BE UNDONE !!${C_NC}"
ui_divider "$C_RED"
echo ""
echo -e "  ${C_CYAN}Stack:${C_NC}            ${STACK_NAME}"
echo -e "  ${C_CYAN}App:${C_NC}              ${APP_NAME}"
echo -e "  ${C_CYAN}Config directory:${C_NC} ${APP_DIR}"
echo ""
echo -e "  ${C_YELLOW}What will happen after the next Git sync (~5 min):${C_NC}"
echo -e "    1. Container '${APP_NAME}' will be ${C_RED}STOPPED${C_NC}"
echo -e "    2. Container '${APP_NAME}' will be ${C_RED}REMOVED${C_NC}"
echo -e "    3. All host data at ${C_RED}/opt/appdata/${STACK_NAME}/${APP_NAME}${C_NC} will be ${C_RED}DELETED${C_NC}"
echo ""

if ! ui_confirm "Are you sure you want to proceed?"; then
    ui_info "Aborted."
    SUCCESS=1
    exit 0
fi

echo ""
echo -e "${UI_INDENT}${C_RED}Final confirmation required.${C_NC}"
CONFIRM2=$(ui_input_required "Type the app name to confirm deletion" "${APP_NAME}") || { ui_info "Aborted."; SUCCESS=1; exit 0; }
# Trim accidental leading/trailing whitespace before comparing
CONFIRM2="$(ui_trim "$CONFIRM2")"
if [[ "$CONFIRM2" != "$APP_NAME" ]]; then
    ui_info "App name did not match. Aborted."
    SUCCESS=1
    exit 0
fi

ui_step "Removing ${APP_DIR} from Git..."
git rm -rf "${APP_DIR}" > /dev/null

ui_step "Committing and pushing changes..."
git commit -m "feat(${STACK_NAME}): remove ${APP_NAME} and trigger garbage collection" > /dev/null
git push > /dev/null

SUCCESS=1

echo ""
ui_success "App '${APP_NAME}' has been removed."
ui_info "Within 5 minutes, the GitOps cronjob will execute Garbage Collection on the node."
