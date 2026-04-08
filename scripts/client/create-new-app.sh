#!/usr/bin/env bash
# Script Name: create-new-app.sh
# Description: Generates a new application template within an existing stack using shared library functions.

set -euo pipefail

# Source the shared library
source "$(dirname "$0")/lib-stack.sh"

# Ensure we are running from the root of the repo
require_repo_root

echo "=== Add a New App to an Existing Stack ==="

# Select an existing stack using the library function
STACK_NAME=$(prompt_stack_selection)

if [[ -z "$STACK_NAME" ]]; then
    echo "Error: No stack selected or available."
    exit 1
fi

echo "Selected stack: ${STACK_NAME}"
echo ""

# Prompt for the new app name
while true; do
    read -r -p "Enter the new app name: " APP_NAME
    if [[ -n "$APP_NAME" ]]; then
        # Check if the directory already exists
        if [[ -d "apps/${STACK_NAME}/${APP_NAME}" ]]; then
            echo "Error: App '${APP_NAME}' already exists in stack '${STACK_NAME}'. Please choose a different name."
        else
            break
        fi
    else
        echo "Error: App name cannot be empty."
    fi
done

# Prompt for Docker usage
read -r -p "Will this app use Docker? (y/n) [y]: " USE_DOCKER
USE_DOCKER=${USE_DOCKER:-y}

# Generate the app using the shared function
echo "Creating infrastructure template for app '${APP_NAME}' in stack '${STACK_NAME}'..."
generate_app "${STACK_NAME}" "${APP_NAME}" "${USE_DOCKER}"

echo ""
echo "App generation completed."
echo "You can now edit the docker-compose.yml and .env files directly."
echo "When you run 'git add', Git and SOPS will invisibly encrypt the .env files for you."
