#!/usr/bin/env bash
# =============================================================================
# setup-latch.sh (LXC)
# Lightweight latch installer wrapper for Proxmox LXCs.
#
# This script intentionally does NOT compile from source and does NOT install
# Rust toolchains in the container. It expects a pre-built Debian-12-compatible
# binary to be pushed by HOST (default: /root/latch).
#
# Usage: ./setup-latch.sh [--verify-only] [--with-pass]
# =============================================================================

set -euo pipefail

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}"

VERIFY_ONLY="${1:-}"
WITH_PASS="false"
if [[ "${1:-}" == "--with-pass" || "${2:-}" == "--with-pass" ]]; then
    WITH_PASS="true"
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}i${NC} $*"
}

log_success() {
    echo -e "${GREEN}OK${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}WARN${NC} $*"
}

log_error() {
    echo -e "${RED}ERR${NC} $*" >&2
}

detect_os() {
    if [[ -f /etc/os-release ]]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        OS="${ID:-unknown}"
    else
        OS="unknown"
    fi
}

resolve_binary_source() {
    local source="${LATCH_BINARY_SOURCE:-/root/latch}"

    if [[ -f "$source" ]]; then
        printf '%s\n' "$source"
        return 0
    fi

    if [[ -f /tmp/latch ]]; then
        printf '%s\n' "/tmp/latch"
        return 0
    fi

    if [[ -f /root/latch-linux-x86_64-lxc.tar.gz ]]; then
        printf '%s\n' "/root/latch-linux-x86_64-lxc.tar.gz"
        return 0
    fi

    return 1
}

extract_if_archive() {
    local src="$1"
    local out="$2"

    if [[ "$src" != *.tar.gz ]]; then
        cp "$src" "$out"
        chmod 755 "$out"
        return 0
    fi

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' RETURN

    local entry
    entry="$(tar -tzf "$src" | grep -E '(^|/)latch$' | head -1 || true)"
    if [[ -z "$entry" ]]; then
        return 1
    fi

    tar -xzf "$src" -C "$tmp_dir" "$entry"
    if [[ ! -f "$tmp_dir/$entry" ]]; then
        return 1
    fi

    cp "$tmp_dir/$entry" "$out"
    chmod 755 "$out"
    return 0
}

verify_runtime_compat() {
    if command -v ldd >/dev/null 2>&1; then
        local ldd_out
        ldd_out="$(ldd /usr/local/bin/latch 2>&1 || true)"
        if echo "$ldd_out" | grep -qi "not found"; then
            log_error "latch has unresolved runtime deps: $ldd_out"
            return 1
        fi
    fi

    if ! /usr/local/bin/latch --version >/dev/null 2>&1; then
        log_error "latch binary failed runtime check (possibly glibc mismatch)"
        return 1
    fi

    return 0
}

install_latch_cli() {
    local source
    if ! source="$(resolve_binary_source)"; then
        log_error "No prebuilt latch binary found. Expected /root/latch or /tmp/latch."
        log_error "Push binary first: pct push <vmid> <host-binary-path> /root/latch"
        return 1
    fi

    local staged="/tmp/latch-install-src"
    if ! extract_if_archive "$source" "$staged"; then
        log_error "Failed to stage latch binary from $source"
        return 1
    fi

    install -m 755 "$staged" /usr/local/bin/latch
    if [[ -d /usr/bin ]]; then
        ln -sfn /usr/local/bin/latch /usr/bin/latch || true
    fi

    if ! verify_runtime_compat; then
        return 1
    fi

    log_success "latch CLI installed from $source"
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

verify_setup() {
    log_info "Verifying setup..."

    local success=true

    if [[ -x /usr/local/bin/latch ]]; then
        if verify_runtime_compat; then
            log_success "latch CLI available (/usr/local/bin/latch)"
        else
            success=false
        fi
    elif command -v latch >/dev/null 2>&1; then
        if latch --version >/dev/null 2>&1; then
            log_success "latch CLI available ($(command -v latch))"
        else
            log_error "latch exists but is not runnable"
            success=false
        fi
    else
        log_error "latch CLI not found"
        success=false
    fi

    if command -v pass >/dev/null 2>&1 || command -v secret-tool >/dev/null 2>&1; then
        log_success "optional keyring backend available"
    elif [[ -n "${LATCH_PAT:-}" && -n "${LATCH_KEY:-}" ]]; then
        log_success "env fallback available via LATCH_PAT/LATCH_KEY"
    else
        log_warning "No keyring backend detected and LATCH_PAT/LATCH_KEY are not exported in this shell"
    fi

    if [[ "$success" == "true" ]]; then
        log_success "Setup verification passed"
        return 0
    fi

    log_error "Setup verification failed"
    return 1
}

main() {
    log_info "Latch binary setup for LXC container"

    detect_os

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
        log_info "Skipping pass/keyring install; env-backed headless mode is default"
    fi

    verify_setup || exit 1

    log_success "Latch setup complete"
    log_info "Credentials remain persistent via /root/.env and /etc/environment once injected by HOST"
}

main
