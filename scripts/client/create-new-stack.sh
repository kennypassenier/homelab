#!/usr/bin/env bash
# Script Name: create-new-stack.sh
# Description: Generates a new application stack template using shared library functions.

set -euo pipefail

# Source the shared library
source "$(dirname "$0")/lib-stack.sh"

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

if [[ -z "$STACK_NAME" ]]; then
    read -r -p "Enter the name of the new stack (LXC container): " STACK_NAME
fi

if [[ -z "$STACK_NAME" ]]; then
    echo "Error: Stack name cannot be empty."
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

echo "Creating infrastructure template for stack ${STACK_NAME}..."

APPS=()

while true; do
    echo ""
    read -r -p "Enter app name for this stack (leave empty to finish): " APP_NAME
    if [[ -z "$APP_NAME" ]]; then
        break
    fi

    generate_app "${STACK_NAME}" "${APP_NAME}" "${USE_DOCKER}"
    APPS+=("${APP_NAME}")
done

# Generate central Watchtower for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_WATCHTOWER" =~ ^[Yy]$ ]] && [ ${#APPS[@]} -gt 0 ]; then
    generate_watchtower "${STACK_NAME}"
fi

# Generate central Promtail for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_PROMTAIL" =~ ^[Yy]$ ]]; then
    generate_promtail "${STACK_NAME}"
fi

echo ""
echo "Stack generation completed."
echo "You can now edit the docker-compose.yml and .env files directly."
echo "When you run 'git add', Git and SOPS will invisibly encrypt the .env files for you."
