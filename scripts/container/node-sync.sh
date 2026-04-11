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

# Acquire an exclusive lock to prevent concurrent sync runs. If a previous sync is still
# active (e.g. a slow image pull that exceeds the 5-minute cron interval), this instance
# exits cleanly instead of racing against it and causing undefined compose state.
LOCK_FILE="/var/lock/node-sync-${STACK_NAME}.lock"
exec 200>"${LOCK_FILE}"
flock -n 200 || { echo "[$(date -Iseconds)] Another sync is already running for '${STACK_NAME}'. Skipping this cycle."; exit 0; }

cd "${GITOPS_DIR}" || exit 1
git fetch origin main
git checkout main
git pull origin main

if [[ -d "${STACK_DIR}" ]]; then
    cd "${STACK_DIR}" || exit 1

    # Run pre-sync scripts if they exist.
    # Process substitution (<(...)) is used instead of a pipe to avoid running the loop
    # body in a subshell. A piped while-loop does not propagate set -euo pipefail, meaning
    # a failing pre-sync.sh would be silently ignored rather than aborting the sync.
    while IFS= read -r pre_sync_file; do
        echo "Running pre-sync script: $pre_sync_file"
        bash "$pre_sync_file"
    done < <(find . -name "pre-sync.sh" -type f)

    # Apply all docker-compose configurations.
    # Same rationale for process substitution — errors in the loop body must propagate.
    while IFS= read -r compose_file; do
        app_dir=$(dirname "$compose_file")
        echo "Syncing $app_dir..."
        cd "$app_dir"
        docker compose pull -q
        docker compose up -d --remove-orphans
        # Health check: warn about any services that exited immediately after deployment.
        # Does not abort the sync — surfaces failures in the cron log for faster debugging.
        exited_services=$(docker compose ps --services --filter status=exited 2>/dev/null || true)
        if [[ -n "$exited_services" ]]; then
            echo "WARNING: Services not running in '${app_dir}': ${exited_services}"
            echo "WARNING: Run 'docker compose logs' in ${app_dir} to investigate."
        fi
        cd - > /dev/null
    done < <(find . -maxdepth 2 -type f \( -name "docker-compose.yml" -o -name "compose.yaml" \))

    # Garbage Collection: Remove orphaned stacks and their data
    if [[ -d "/appdata/${STACK_NAME}" ]]; then
        for app_data_dir in /appdata/${STACK_NAME}/*; do
            if [[ -d "$app_data_dir" ]]; then
                app_name=$(basename "$app_data_dir")
                # If the app folder no longer exists in Git, it's an orphan
                if [[ ! -d "${STACK_DIR}/${app_name}" ]]; then
                    echo "Garbage Collection: App '${app_name}' no longer in Git. Removing..."
                    # Use the compose project name (defaults to the app folder name) to
                    # gracefully stop ALL containers in that project. This is more reliable
                    # than 'docker stop <name>', which fails silently when the container name
                    # differs from the folder name (e.g. 'watchtower-media' in 'watchtower/').
                    docker compose -p "${app_name}" down 2>/dev/null || {
                        # Fallback: if compose project metadata is already gone, stop by name
                        docker stop "${app_name}" 2>/dev/null || true
                        docker rm "${app_name}" 2>/dev/null || true
                    }
                    # Safely delete the remaining app configuration data from the host mount
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
