#!/usr/bin/env bash
# Minimal sparse-setup.sh: only performs sparse checkout, no SOPS/Age, no $3, no secrets logic.
set -euo pipefail
REPO_URL="https://github.com/kennypassenier/homelab.git"
STACK_DIR="stacks/$1"
GITHUB_PAT="$2"

export GIT_TERMINAL_PROMPT=0
AUTH_REPO_URL=$(echo "$REPO_URL" | sed "s|https://|https://$GITHUB_PAT@|g")

rm -rf /opt/gitops
mkdir -p /opt/gitops
cd /opt/gitops || exit 1
git clone --no-checkout --filter=blob:none "$AUTH_REPO_URL" .
git sparse-checkout init --cone
git sparse-checkout set "$STACK_DIR" "scripts"
git checkout main
