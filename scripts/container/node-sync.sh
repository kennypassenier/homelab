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
STACK_DIR="${GITOPS_DIR}/apps/${STACK_NAME}"

cd "${GITOPS_DIR}" || exit 1
git fetch origin main
git checkout main
git pull origin main

if [[ -d "${STACK_DIR}" ]]; then
    cd "${STACK_DIR}" || exit 1

    # Run pre-sync scripts if they exist
    find . -name "pre-sync.sh" -type f -executable -exec {} \;

    # Apply all docker-compose configurations
    find . -maxdepth 2 -type f \( -name "docker-compose.yml" -o -name "compose.yaml" \) | while read -r compose_file; do
        app_dir=$(dirname "$compose_file")
        echo "Syncing $app_dir..."
        cd "$app_dir"
        docker compose pull -q
        docker compose up -d --remove-orphans
        cd - > /dev/null
    done
else
    echo "Stack ${STACK_NAME} not found in Git."
fi
