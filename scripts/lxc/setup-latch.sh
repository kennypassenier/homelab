#!/usr/bin/env bash
# =============================================================================
# setup-latch.sh (LXC)
# Install latch CLI and keyring in LXC container for credential sync.
#
# Usage: ./setup-latch.sh [--verify-only]
#
# Exit codes:
#   0 = setup successful
#   1 = setup failed
# =============================================================================

set -e

VERIFY_ONLY="${1:-}"

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

# Install latch CLI
install_latch_cli() {
    if command -v latch &> /dev/null; then
        log_success "latch CLI already installed ($(latch --version 2>/dev/null || echo 'unknown version'))"
        return 0
    fi

    log_info "Installing latch CLI..."

    case "$OS" in
        debian | ubuntu)
            apt-get update -qq
            apt-get install -y latch || {
                log_warning "latch not in apt; attempting cargo install"
                if command -v cargo &> /dev/null; then
                    cargo install latch --locked
                else
                    log_error "latch not available via apt and cargo not found"
                    return 1
                fi
            }
            ;;
        alpine)
            apk update
            apk add latch || {
                log_warning "latch not in apk; attempting cargo install"
                if command -v cargo &> /dev/null; then
                    cargo install latch --locked
                else
                    log_error "latch not available via apk and cargo not found"
                    return 1
                fi
            }
            ;;
        *)
            log_warning "Unknown OS ($OS); attempting cargo install"
            if command -v cargo &> /dev/null; then
                cargo install latch --locked
            else
                log_error "Cannot install latch on $OS without cargo"
                return 1
            fi
            ;;
    esac

    log_success "latch CLI installed"
}

# Install OS keyring
install_keyring() {
    log_info "Setting up OS keyring..."

    # Check if a keyring is already available
    if command -v pass &> /dev/null; then
        log_success "pass (password manager) already installed"
        return 0
    fi

    if command -v secret-tool &> /dev/null; then
        log_success "secret-tool (gnome-keyring) already available"
        return 0
    fi

    # Try to install pass (most portable)
    case "$OS" in
        debian | ubuntu)
            apt-get update -qq
            apt-get install -y pass gnupg || {
                log_warning "Could not install pass; trying other keyrings"
                apt-get install -y gnome-keyring || true
            }
            ;;
        alpine)
            apk update
            apk add pass gnupg || {
                log_warning "Could not install pass; trying other keyrings"
                true
            }
            ;;
        *)
            log_warning "Cannot auto-install keyring on $OS; manual setup may be required"
            return 1
            ;;
    esac

    log_success "OS keyring installed"
}

# Initialize pass GPG setup (if needed)
init_pass_gpg() {
    if ! command -v pass &> /dev/null; then
        return 0
    fi

    # Check if GPG key exists
    if ! gpg --list-secret-keys --quiet &> /dev/null; then
        log_info "Initializing GPG key for pass..."
        # Generate a temporary GPG key for automated keyring operations
        gpg --batch --gen-key <<EOF
Key-Type: RSA
Key-Length: 2048
Name-Real: Homelab LXC Daemon
Name-Email: lxc-daemon@homelab.local
Expire-Date: 0
%no-protection
EOF
        log_success "GPG key generated"
    else
        log_success "GPG key already exists"
    fi

    # Initialize pass if not already done
    if [[ ! -d ~/.password-store ]]; then
        log_info "Initializing pass password store..."
        pass init "$(gpg --list-secret-keys --quiet | grep uid | head -1 | sed 's/.*<\(.*\)>.*/\1/')" || {
            log_warning "Could not auto-init pass; manual init may be required"
        }
    fi
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

    # Check keyring
    if command -v pass &> /dev/null || command -v secret-tool &> /dev/null; then
        log_success "✓ OS keyring available"
    else
        log_warning "⚠ No keyring backend found (but latch may provide fallback)"
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
    log_info "Latch + Keyring setup for LXC container"

    detect_os || exit 1

    if [[ "$VERIFY_ONLY" == "--verify-only" ]]; then
        verify_setup
        exit $?
    fi

    install_latch_cli || exit 1
    install_keyring || {
        log_warning "Keyring installation failed; continuing anyway"
    }
    init_pass_gpg || {
        log_warning "GPG initialization failed; continuing anyway"
    }
    verify_setup || exit 1

    log_success "Latch setup complete!"
    log_info "Credentials are now ready for sync from CLIENT via: make docker --with-secrets"
}

main
