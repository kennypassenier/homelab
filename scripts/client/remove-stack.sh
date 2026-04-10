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

ui_info "=== Remove an Entire Stack ==="

# Select an existing stack using the foolproof interactive library function
STACK_NAME=$(prompt_stack_selection)

if [[ -z "$STACK_NAME" ]]; then
    ui_error "No stack selected or available."
    exit 1
fi

STACK_DIR="stacks/${STACK_NAME}"

echo ""
echo -e "${C_RED}================================================================${C_NC}"
echo -e "${C_RED}WARNING: You are about to completely destroy the ENTIRE STACK '${STACK_NAME}'!${C_NC}"
echo -e "${C_RED}================================================================${C_NC}"
ui_info "This will delete the Git configuration directory: ${STACK_DIR}"
ui_info "Once synced, the node-sync.sh script on the LXC container will automatically:"
ui_info " 1. STOP all containers in the stack"
ui_info " 2. REMOVE all containers in the stack"
ui_info " 3. DELETE all their data from the host!"
echo ""

# First Confirmation: simple y/n
read -r -p "Are you sure you want to proceed? (y/N): " CONFIRM1
if [[ ! "$CONFIRM1" =~ ^[Yy]$ ]]; then
    ui_info "Aborted."
    SUCCESS=1
    exit 0
fi

echo ""
echo -e "${C_RED}Final confirmation required.${C_NC}"
# Second Confirmation: explicitly type the stack name
read -r -p "Are you ABSOLUTELY sure you want to delete '${STACK_NAME}'? Type the stack name to confirm: " CONFIRM2
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
