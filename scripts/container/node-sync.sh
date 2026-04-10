#!/usr/bin/env bash
# Script Name: node-sync.sh
# Usage: ./node-sync.sh [-h] <STACK_NAME>
set -euo pipefail

show_help() {
    echo "Usage: $0 [-h] <STACK_NAME>"
    echo "  -h    Show this help message"
    exit 0
}

while getopts "h" opt; do
    case "$opt" in
        h) show_help ;;
        *) show_help ;;
    esac
done
shift $((OPTIND-1))

if [[ $# -ne 1 ]]; then
    show_help
fi

STACK_NAME="$1"
GITOPS_DIR="/opt/gitops"
STACK_DIR="${GITOPS_DIR}/stacks/${STACK_NAME}"

cd "${GITOPS_DIR}" || exit 1
git fetch origin main
git checkout main
git pull origin main

if [[ -d "${STACK_DIR}" ]]; then
    cd "${STACK_DIR}" || exit 1

    # Run pre-sync scripts if they exist
    find . -name "pre-sync.sh" -type f | while read -r pre_sync_file; do
        echo "Running pre-sync script: $pre_sync_file"
        bash "$pre_sync_file"
    done

    # Apply all docker-compose configurations
    find . -maxdepth 2 -type f \( -name "docker-compose.yml" -o -name "compose.yaml" \) | while read -r compose_file; do
        app_dir=$(dirname "$compose_file")
        echo "Syncing $app_dir..."
        cd "$app_dir"
        docker compose pull -q
        docker compose up -d --remove-orphans
        cd - > /dev/null
    done

    # Garbage Collection: Remove orphaned stacks and their data
    if [[ -d "/appdata/${STACK_NAME}" ]]; then
        for app_data_dir in /appdata/${STACK_NAME}/*; do
            if [[ -d "$app_data_dir" ]]; then
                app_name=$(basename "$app_data_dir")
                # If the app folder no longer exists in Git, it's an orphan
                if [[ ! -d "${STACK_DIR}/${app_name}" ]]; then
                    echo "Garbage Collection: App '${app_name}' no longer in Git. Removing..."
                    # Attempt to stop and remove the container by app name
                    docker stop "${app_name}" 2>/dev/null || true
                    docker rm "${app_name}" 2>/dev/null || true
                    # Safely delete the remaining app configuration data
                    rm -rf "$app_data_dir"
                    echo "Garbage Collection: Removed container and data for '${app_name}'."
                fi
            fi
        done
    fi
else
    echo "Stack ${STACK_NAME} not found in Git."
fi

# One-time cleanup of legacy bootstrap artifacts
if [[ -f "/root/sparse-setup.sh" ]]; then
    echo "Cleaning up legacy bootstrap artifact: /root/sparse-setup.sh"
    rm -f "/root/sparse-setup.sh"
fi

if [[ -d "/tmp/age" ]]; then
    echo "Cleaning up legacy bootstrap artifact: /tmp/age"
    rm -rf "/tmp/age"
fi
