#!/usr/bin/env bash
# =============================================================================
# Stack Pre-Sync Hook Template
# =============================================================================
# This script runs before every LXC sync and should:
# 1. Create required /appdata directories
# 2. Pull secrets via latch (sparse mode preferred)
# 3. Prepare any other pre-deploy setup
#
# Location: stacks/<stack_name>/pre-sync.sh
# Invoked by: LXC daemon before `docker compose pull && docker compose up`
# =============================================================================

set -euo pipefail

STACK_NAME="my-stack"
APPDATA_ROOT="/appdata/${STACK_NAME}"

# ── Create all required app data directories ─────────────────────────────────

mkdir -p "${APPDATA_ROOT}/app-name-1/config"
mkdir -p "${APPDATA_ROOT}/app-name-2/config"

# ── Pull secrets using Latch (sparse mode) ──────────────────────────────────
# Sparse mode ensures only existing directories receive .env files,
# which prevents accidental creation of unmanaged directories.

if command -v latch &>/dev/null; then
    echo "Pulling secrets for ${STACK_NAME} (sparse mode)..."
    latch pull --sparse --env prod || {
        echo "Warning: latch pull failed; continuing with any cached .env files" >&2
    }
else
    echo "Warning: latch not found; skipping secret sync" >&2
fi

# ── Custom initialization (optional) ────────────────────────────────────────
# Add stack-specific setup here if needed (e.g., config file generation,
# database initialization, etc.)

echo "Pre-sync hook completed for ${STACK_NAME}"
