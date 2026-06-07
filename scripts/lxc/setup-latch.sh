#!/usr/bin/env bash
# =============================================================================
# setup-latch.sh (LXC)
# Install latch CLI from the latest GitHub release and configure a guarded
# updater for headless LXC containers.
#
# Usage: ./setup-latch.sh [--verify-only] [--with-pass]
#
# Exit codes:
#   0 = setup successful
#   1 = setup failed
# =============================================================================

set -e

VERIFY_ONLY="${1:-}"
WITH_PASS="false"
if [[ "${1:-}" == "--with-pass" || "${2:-}" == "--with-pass" ]]; then
    WITH_PASS="true"
fi

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}ℹ${NC} $*"
}

log_success() {
    echo -e "${GREEN}✓${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $*"
}

log_error() {
    echo -e "${RED}✗${NC} $*" >&2
}

# Detect OS and package manager
detect_os() {
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        OS=$ID
        VERSION=$VERSION_ID
    else
        log_error "Cannot detect OS"
        return 1
    fi
}

# Install latch CLI from the latest GitHub release
install_latch_cli() {
    install_release_prereqs
    write_release_helper
    /usr/local/bin/install-latch-release
    write_guarded_update_helper
    install_update_timer
    log_success "latch CLI installed ($(latch --version 2>/dev/null || echo 'unknown version'))"
}

install_release_prereqs() {
    case "$OS" in
        debian | ubuntu)
            apt-get update -qq
            apt-get install -y -qq curl jq tar ca-certificates
            ;;
        alpine)
            apk update >/dev/null
            apk add curl jq tar ca-certificates
            ;;
        *)
            log_warning "Unknown OS ($OS); assuming curl/jq/tar are already present"
            ;;
    esac
}

write_release_helper() {
    cat > /usr/local/bin/install-latch-release <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ -f /root/.env ]]; then
    set -a
    # shellcheck disable=SC1091
    . /root/.env
    set +a
fi

REPO="${LATCH_UPDATE_REPO:-kennypassenier/latch-rs}"
ASSET="${LATCH_UPDATE_ASSET:-latch-linux-x86_64.tar.gz}"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

release_json="$(curl -fsSL "$API_URL")"
tag="$(printf '%s' "$release_json" | jq -r '.tag_name')"
asset_url="$(printf '%s' "$release_json" | jq -r --arg asset "$ASSET" '.assets[] | select(.name == $asset) | .browser_download_url' | head -1)"

if [[ -z "$asset_url" || "$asset_url" == "null" ]]; then
    echo "Latch release ${tag} missing asset ${ASSET}" >&2
    exit 1
fi

current_version=""
if command -v latch >/dev/null 2>&1; then
    current_version="$(latch --version 2>/dev/null | awk 'NR==1{print $NF}')"
fi

latest_version="${tag#v}"
if [[ -n "$current_version" && "$current_version" == "$latest_version" && -x /usr/local/bin/latch ]]; then
    echo "Latch already current at ${current_version}"
    exit 0
fi

curl -fsSL "$asset_url" -o "$TMP_DIR/latch.tar.gz"
tar -xzf "$TMP_DIR/latch.tar.gz" -C "$TMP_DIR"

if [[ ! -f "$TMP_DIR/latch" ]]; then
    echo "Latch archive did not contain binary 'latch'" >&2
    exit 1
fi

install -m 755 "$TMP_DIR/latch" /usr/local/bin/latch
echo "Installed latch ${latest_version} to /usr/local/bin/latch"
EOF
    chmod 755 /usr/local/bin/install-latch-release
}

write_guarded_update_helper() {
    cat > /usr/local/bin/latch-update-safe <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ -f /root/.env ]]; then
    set -a
    # shellcheck disable=SC1091
    . /root/.env
    set +a
fi

STATE_DIR="/var/lib/homelab"
STAMP_FILE="${STATE_DIR}/latch-update.last"
INTERVAL_SECS="${LATCH_UPDATE_INTERVAL_SECS:-86400}"
FORCE="${1:-}"

mkdir -p "$STATE_DIR"
now="$(date +%s)"
last="0"
if [[ -f "$STAMP_FILE" ]]; then
    last="$(cat "$STAMP_FILE" 2>/dev/null || echo 0)"
fi

if [[ "$FORCE" != "--force" ]] && (( now - last < INTERVAL_SECS )); then
    exit 0
fi

/usr/local/bin/install-latch-release
date +%s > "$STAMP_FILE"
EOF
    chmod 755 /usr/local/bin/latch-update-safe
}

install_update_timer() {
    cat > /etc/systemd/system/latch-update.service <<'EOF'
[Unit]
Description=Guarded latch binary updater
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/latch-update-safe
EOF

    cat > /etc/systemd/system/latch-update.timer <<'EOF'
[Unit]
Description=Daily guarded latch binary update check

[Timer]
OnBootSec=15m
OnUnitActiveSec=1d
Persistent=true

[Install]
WantedBy=timers.target
EOF

    systemctl daemon-reload
    systemctl enable --now latch-update.timer >/dev/null 2>&1 || true
}

install_optional_pass() {
    log_info "Installing optional pass backend..."
    case "$OS" in
        debian | ubuntu)
            apt-get update -qq
            apt-get install -y -qq pass gnupg
            ;;
        alpine)
            apk update >/dev/null
            apk add pass gnupg
            ;;
        *)
            log_warning "Cannot auto-install pass on $OS"
            return 1
            ;;
    esac
    log_success "pass installed (optional backend)"
}

# Verify setup
verify_setup() {
    log_info "Verifying setup..."

    local success=true

    # Check latch
    if command -v latch &> /dev/null; then
        log_success "✓ latch CLI available"
    else
        log_error "✗ latch CLI not found"
        success=false
    fi

    # Check optional keyring / env fallback
    if command -v pass &> /dev/null || command -v secret-tool &> /dev/null; then
        log_success "✓ optional keyring backend available"
    elif [[ -n "${LATCH_PAT:-}" && -n "${LATCH_KEY:-}" ]]; then
        log_success "✓ headless env fallback available via LATCH_PAT/LATCH_KEY"
    else
        log_warning "⚠ No keyring backend detected and LATCH_PAT/LATCH_KEY are not exported in this shell"
    fi

    if [[ "$success" == "true" ]]; then
        log_success "Setup verification passed"
        return 0
    else
        log_error "Setup verification failed"
        return 1
    fi
}

main() {
    log_info "Latch binary setup for LXC container"

    detect_os || exit 1

    if [[ "$VERIFY_ONLY" == "--verify-only" ]]; then
        verify_setup
        exit $?
    fi

    install_latch_cli || exit 1
    if [[ "$WITH_PASS" == "true" ]]; then
        install_optional_pass || {
            log_warning "pass installation failed; continuing with env-backed mode"
        }
    else
        log_info "Skipping pass/keyring install; env-backed headless mode is the default for LXCs"
    fi
    verify_setup || exit 1

    log_success "Latch setup complete!"
    log_info "Credentials remain persistent via /root/.env and /etc/environment once injected by HOST"
}

main
