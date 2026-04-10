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

ui_info "=== Remove an App from an Existing Stack ==="

# Select an existing stack using the library function
STACK_NAME=$(prompt_stack_selection)

if [[ -z "$STACK_NAME" ]]; then
    ui_error "No stack selected or available."
    exit 1
fi

ui_step "Selected stack: ${STACK_NAME}"
echo ""

# Prompt for the app name to remove
read -r -p "Enter the app name to completely remove from ${STACK_NAME}: " APP_NAME

if [[ -z "$APP_NAME" ]]; then
    ui_error "App name cannot be empty."
    exit 1
fi

APP_DIR="apps/${STACK_NAME}/${APP_NAME}"

if [[ ! -d "$APP_DIR" ]]; then
    ui_error "App '${APP_NAME}' does not exist in Git (${APP_DIR})."
    exit 1
fi

echo ""
ui_warning "WARNING: This will delete the Git configuration for '${APP_NAME}'."
ui_info "Once synced, the node-sync.sh script on the LXC container will automatically"
ui_info "STOP the container, REMOVE the container, and DELETE all its data from the host!"
echo ""

read -r -p "Are you absolutely sure you want to proceed? (y/N): " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    ui_info "Aborted."
    SUCCESS=1
    exit 0
fi

ui_step "Removing ${APP_DIR} from Git..."
git rm -r "${APP_DIR}" > /dev/null

ui_step "Committing and pushing changes..."
git commit -m "feat(${STACK_NAME}): remove ${APP_NAME} and trigger garbage collection" > /dev/null
git push > /dev/null

SUCCESS=1

echo ""
ui_success "App '${APP_NAME}' has been removed."
ui_info "Within 5 minutes, the GitOps cronjob will execute Garbage Collection on the node."
