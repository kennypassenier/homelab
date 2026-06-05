#!/usr/bin/env bash
# Render per-service .env files from a central bundle file.
#
# Usage:
#   ./scripts/shared/sync-env-bundle.sh --bundle config/env.bundle

set -euo pipefail

BUNDLE_FILE="config/env.bundle"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bundle)
            BUNDLE_FILE="$2"
            shift 2
            ;;
        -h|--help)
            cat <<'EOF'
Usage: sync-env-bundle.sh [--bundle <path>]

Reads a central env bundle and writes:
- client-app/.env
- host-daemon/.env
- lxc-daemon/.env
- scripts/host/.env
EOF
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

if [[ ! -f "$BUNDLE_FILE" ]]; then
    echo "Bundle file not found: $BUNDLE_FILE"
    exit 1
fi

set -a
# shellcheck source=/dev/null
source "$BUNDLE_FILE"
set +a

mkdir -p client-app host-daemon lxc-daemon scripts/host

cat > client-app/.env <<EOF
# Generated from $BUNDLE_FILE
LXC_API_IP=${LXC_API_IP:-127.0.0.1}
LXC_API_TOKEN=${LXC_API_TOKEN:-}
LATCH_PAT=${LATCH_PAT:-}
LATCH_KEY=${LATCH_KEY:-}
OPNSENSE_BASE_URL=${OPNSENSE_BASE_URL:-}
OPNSENSE_API_KEY=${OPNSENSE_API_KEY:-}
OPNSENSE_API_SECRET=${OPNSENSE_API_SECRET:-}
OPNSENSE_TLS_INSECURE=${OPNSENSE_TLS_INSECURE:-false}
LXC_CLOUDFLARED_IP=${LXC_CLOUDFLARED_IP:-10.10.10.9}
LXC_DOWNLOADER_IP=${LXC_DOWNLOADER_IP:-10.10.10.5}
LXC_GATEWAY_IP=${LXC_GATEWAY_IP:-10.10.10.8}
LXC_MEDIA_IP=${LXC_MEDIA_IP:-10.10.10.6}
LXC_MONITORING_IP=${LXC_MONITORING_IP:-10.10.10.7}
LXC_PAPERLESS_IP=${LXC_PAPERLESS_IP:-10.10.10.4}
LXC_VIKUNJA_IP=${LXC_VIKUNJA_IP:-10.10.10.11}
EOF

cat > host-daemon/.env <<EOF
# Generated from $BUNDLE_FILE
HOST_UPDATE_REPO=${HOST_UPDATE_REPO:-kennypassenier/homelab}
HOST_UPDATE_ASSET=${HOST_UPDATE_ASSET:-HOST-linux-x86_64-unknown-linux-gnu}
HOST_UPDATE_TOKEN=${HOST_UPDATE_TOKEN:-}
LATCH_PAT=${LATCH_PAT:-}
LATCH_KEY=${LATCH_KEY:-}
LXC_CLOUDFLARED_IP=${LXC_CLOUDFLARED_IP:-10.10.10.9}
LXC_DOWNLOADER_IP=${LXC_DOWNLOADER_IP:-10.10.10.5}
LXC_GATEWAY_IP=${LXC_GATEWAY_IP:-10.10.10.8}
LXC_MEDIA_IP=${LXC_MEDIA_IP:-10.10.10.6}
LXC_MONITORING_IP=${LXC_MONITORING_IP:-10.10.10.7}
LXC_PAPERLESS_IP=${LXC_PAPERLESS_IP:-10.10.10.4}
LXC_VIKUNJA_IP=${LXC_VIKUNJA_IP:-10.10.10.11}
RESTIC_REPO_BASE=${RESTIC_REPO_BASE:-/backups}
RCLONE_CONFIG_FILE=${RCLONE_CONFIG_FILE:-}
EOF

cat > lxc-daemon/.env <<EOF
# Generated from $BUNDLE_FILE
STACK_NAME=${STACK_NAME:-media}
STACK_IP=${STACK_IP:-}
GITOPS_REPO_URL=${GITOPS_REPO_URL:-}
GITOPS_REPO_TOKEN=${GITOPS_REPO_TOKEN:-}
LXC_API_TOKEN=${LXC_API_TOKEN:-}
EOF

cat > scripts/host/.env <<EOF
# Generated from $BUNDLE_FILE
GITHUB_USERNAME=${GITHUB_USERNAME:-}
GITHUB_PAT=${GITHUB_PAT:-}
LATCH_PAT=${LATCH_PAT:-}
LATCH_KEY=${LATCH_KEY:-}
AGE_PASSPHRASE=${AGE_PASSPHRASE:-}
RESTIC_REPOSITORY=${RESTIC_REPOSITORY:-}
RESTIC_PASSWORD=${RESTIC_PASSWORD:-}
RCLONE_CONFIG_FILE=${RCLONE_CONFIG_FILE:-}
EOF

chmod 600 client-app/.env host-daemon/.env lxc-daemon/.env scripts/host/.env

echo "Generated: client-app/.env"
echo "Generated: host-daemon/.env"
echo "Generated: lxc-daemon/.env"
echo "Generated: scripts/host/.env"
