#!/usr/bin/env bash
# Script Name: node-sync.sh
# Description: Syncs repository state. Transparent Git filters handle decryption automatically.

set -euo pipefail

APP_NAME="${1:-}"
GITOPS_DIR="/opt/gitops"

cd "${GITOPS_DIR}" || exit 1

# Pulling changes automatically invokes the SOPS smudge filter for.env files
git pull origin main

# Execute any pre-sync or setup scripts found in the application directories
echo "Checking for pre-sync setup scripts..."
find "apps/${APP_NAME}" -type f \( -name 'setup.sh' -o -name 'pre-sync.sh' \) -exec sh -c '
    for script_file do
        dir=$(dirname "${script_file}")
        echo "Executing setup script in ${dir}..."
        chmod +x "${script_file}"
        (cd "${dir}" && bash "$(basename "${script_file}")")
    done
' sh {} +

# Recursively locate all container manifest files and align the runtime state
echo "Aligning runtime states with declarative manifests..."
find "apps/${APP_NAME}" -type f \( -name 'docker-compose.yml' -o -name 'compose.yaml' \) -exec sh -c '
    for compose_file do
        dir=$(dirname "${compose_file}")
        echo "Reconciling application stack in ${dir}..."
        cd "${dir}" && docker compose up -d --remove-orphans
        cd - > /dev/null
    done
' sh {} +

echo "Synchronization cycle successfully finalized."
