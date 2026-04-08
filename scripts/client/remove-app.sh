#!/usr/bin/env bash
# Script Name: remove-app.sh
# Description: Removes an app from a stack and commits the deletion to trigger Garbage Collection.

set -euo pipefail

# Source the shared library
source "$(dirname "$0")/lib-stack.sh"

# Ensure we are running from the root of the repo
require_repo_root

echo "=== Remove an App from an Existing Stack ==="

# Select an existing stack using the library function
STACK_NAME=$(prompt_stack_selection)

if [[ -z "$STACK_NAME" ]]; then
    echo "Error: No stack selected or available."
    exit 1
fi

echo "Selected stack: ${STACK_NAME}"
echo ""

# Prompt for the app name to remove
read -r -p "Enter the app name to completely remove from ${STACK_NAME}: " APP_NAME

if [[ -z "$APP_NAME" ]]; then
    echo "Error: App name cannot be empty."
    exit 1
fi

APP_DIR="apps/${STACK_NAME}/${APP_NAME}"

if [[ ! -d "$APP_DIR" ]]; then
    echo "Error: App '${APP_NAME}' does not exist in Git (${APP_DIR})."
    exit 1
fi

echo ""
echo "WARNING: This will delete the Git configuration for '${APP_NAME}'."
echo "Once synced, the node-sync.sh script on the LXC container will automatically"
echo "STOP the container, REMOVE the container, and DELETE all its data from the host!"
echo ""

read -r -p "Are you absolutely sure you want to proceed? (y/N): " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

echo "Removing ${APP_DIR} from Git..."
git rm -r "${APP_DIR}"

echo "Committing and pushing changes..."
git commit -m "feat(${STACK_NAME}): remove ${APP_NAME} and trigger garbage collection"
git push

echo ""
echo "App '${APP_NAME}' has been removed."
echo "Within 5 minutes, the GitOps cronjob will execute Garbage Collection on the node."
