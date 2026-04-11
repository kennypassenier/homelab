#!/usr/bin/env bash
# Script Name: remove-stack.sh
# Description: Removes an entire stack from Git and commits the deletion to trigger Garbage Collection.

set -euo pipefail

# Source the shared libraries
source "$(dirname "$0")/lib/lib-stack.sh"
source "scripts/shared/lib-ui.sh"

# Ensure we are running from the root of the repo
require_repo_root

SUCCESS=0
STACK_DIR=""

# --- Rollback & Error Handling ---
# Reverts local Git deletions if the script fails before a successful push
cleanup_on_error() {
    local exit_code=$?
    if [[ $exit_code -ne 0 && $SUCCESS -eq 0 ]]; then
        echo ""
        ui_error "Stack removal failed unexpectedly! (Exit code: $exit_code)"

        # Rollback: If we deleted the stack directory from Git but failed to commit/push, restore it.
        if [[ -n "$STACK_DIR" && ! -d "$STACK_DIR" ]]; then
            ui_warning "Initiating safety rollback..."
            ui_info "Restoring deleted files from Git..."
            git restore --staged "$STACK_DIR" 2>/dev/null || true
            git restore "$STACK_DIR" 2>/dev/null || true
            ui_success "Rollback complete. Stack configuration restored."
        fi

        echo ""
        ui_step "Troubleshooting tips:"
        ui_info "1. Check your Git repository state (git status)."
        ui_info "2. Ensure you have network access to push to the remote."
        echo ""
    fi
}
trap cleanup_on_error EXIT

ui_section "Remove an Entire Stack"

# Select an existing stack using the foolproof interactive library function
# prompt_stack_selection returns 2 if the user chose Cancel
STACK_NAME=$(prompt_stack_selection) || { ui_info "Cancelled."; SUCCESS=1; exit 0; }

STACK_DIR="stacks/${STACK_NAME}"

# Count the apps in this stack so the user knows the blast radius
APP_COUNT=$(find "$STACK_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l)

# --- Confirmation Summary ---
echo ""
ui_divider "$C_RED"
echo -e "${UI_INDENT}${C_RED}!! DESTRUCTIVE ACTION — THIS CANNOT BE UNDONE !!${C_NC}"
ui_divider "$C_RED"
echo ""
echo -e "  ${C_CYAN}Stack:${C_NC}            ${STACK_NAME}"
echo -e "  ${C_CYAN}Config directory:${C_NC} ${STACK_DIR}"
echo -e "  ${C_CYAN}Apps in stack:${C_NC}    ${APP_COUNT}"
echo ""
echo -e "  ${C_YELLOW}What will happen after the next Git sync (~5 min):${C_NC}"
echo -e "    1. All ${APP_COUNT} container(s) in '${STACK_NAME}' will be ${C_RED}STOPPED${C_NC}"
echo -e "    2. All ${APP_COUNT} container(s) in '${STACK_NAME}' will be ${C_RED}REMOVED${C_NC}"
echo -e "    3. All host data at ${C_RED}/opt/appdata/${STACK_NAME}${C_NC} will be ${C_RED}DELETED${C_NC}"
echo ""

# First Confirmation: simple y/n
if ! ui_confirm "Are you sure you want to proceed?"; then
    ui_info "Aborted."
    SUCCESS=1
    exit 0
fi

echo ""
echo -e "${UI_INDENT}${C_RED}Final confirmation required.${C_NC}"
# Second Confirmation: explicitly type the stack name to prevent accidental deletion
CONFIRM2=$(ui_input_required "Type the stack name to confirm deletion" "${STACK_NAME}") || { ui_info "Aborted."; SUCCESS=1; exit 0; }
# Trim accidental leading/trailing whitespace before comparing
CONFIRM2="$(ui_trim "$CONFIRM2")"
if [[ "$CONFIRM2" != "$STACK_NAME" ]]; then
    ui_info "Stack name did not match. Aborted."
    SUCCESS=1
    exit 0
fi

ui_step "Removing ${STACK_DIR} from Git..."
git rm -r "${STACK_DIR}" > /dev/null

ui_step "Committing and pushing changes..."
git commit -m "feat(core): remove stack ${STACK_NAME} and trigger garbage collection" > /dev/null
git push > /dev/null

# Mark as successful to avoid triggering the error trap
SUCCESS=1

echo ""
ui_success "Stack '${STACK_NAME}' has been removed."
ui_info "Within 5 minutes, the GitOps cronjob will execute Garbage Collection on the node."
