#!/usr/bin/env bash
# =============================================================================
# setup-latch.sh (CLIENT)
# Ensure latch CLI and keyring are available on CLIENT desktop for 
# orchestrating credential sync to LXC containers.
#
# Usage: ./setup-latch.sh [--verify-only]
#
# Exit codes:
#   0 = setup successful
#   1 = setup failed or not required
# =============================================================================

set -e

VERIFY_ONLY="${1:-}"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

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

# Detect Linux distribution
detect_distro() {
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        DISTRO=$ID
        VERSION=$VERSION_ID
    else
        # Fallback for older systems
        if command -v lsb_release &> /dev/null; then
            DISTRO=$(lsb_release -is | tr '[:upper:]' '[:lower:]')
            VERSION=$(lsb_release -rs)
        else
            log_error "Cannot detect distribution"
            return 1
        fi
    fi
}

# Install latch CLI
install_latch_cli() {
    if command -v latch &> /dev/null; then
        log_success "latch CLI already installed ($(latch --version 2>/dev/null || echo 'unknown version'))"
        return 0
    fi

    log_info "Installing latch CLI..."

    case "$DISTRO" in
        debian | ubuntu)
            sudo apt-get update -qq
            sudo apt-get install -y latch || {
                log_warning "latch not in apt; attempting cargo install"
                if command -v cargo &> /dev/null; then
                    cargo install latch --locked
                else
                    log_error "latch not available via apt and cargo not found"
                    return 1
                fi
            }
            ;;
        arch)
            sudo pacman -S --noconfirm latch || {
                log_warning "latch not in pacman; attempting cargo install"
                if command -v cargo &> /dev/null; then
                    cargo install latch --locked
                else
                    log_error "latch not available via pacman and cargo not found"
                    return 1
                fi
            }
            ;;
        fedora | rhel | centos)
            sudo dnf install -y latch || {
                log_warning "latch not in dnf; attempting cargo install"
                if command -v cargo &> /dev/null; then
                    cargo install latch --locked
                else
                    log_error "latch not available via dnf and cargo not found"
                    return 1
                fi
            }
            ;;
        alpine)
            sudo apk add latch || {
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
            log_warning "Unknown distribution ($DISTRO); attempting cargo install"
            if command -v cargo &> /dev/null; then
                cargo install latch --locked
            else
                log_error "Cannot install latch on $DISTRO without cargo"
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

    if command -v kwallet-query &> /dev/null; then
        log_success "kwallet (KDE Wallet) already available"
        return 0
    fi

    # Try to install pass (most portable)
    case "$DISTRO" in
        debian | ubuntu)
            sudo apt-get update -qq
            sudo apt-get install -y pass gnupg || {
                log_warning "Could not install pass; trying other keyrings"
                sudo apt-get install -y gnome-keyring || true
            }
            ;;
        arch)
            sudo pacman -S --noconfirm pass gnupg || {
                log_warning "Could not install pass"
                true
            }
            ;;
        fedora | rhel | centos)
            sudo dnf install -y pass gnupg || {
                log_warning "Could not install pass"
                true
            }
            ;;
        alpine)
            sudo apk add pass gnupg || {
                log_warning "Could not install pass"
                true
            }
            ;;
        *)
            log_warning "Cannot auto-install keyring on $DISTRO"
            return 1
            ;;
    esac

    log_success "OS keyring installed"
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
    if command -v pass &> /dev/null || command -v secret-tool &> /dev/null || command -v kwallet-query &> /dev/null; then
        log_success "✓ OS keyring available"
    else
        log_warning "⚠ No keyring backend found"
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
    log_info "Latch + Keyring setup for CLIENT desktop"

    detect_distro || exit 1

    if [[ "$VERIFY_ONLY" == "--verify-only" ]]; then
        verify_setup
        exit $?
    fi

    install_latch_cli || exit 1
    install_keyring || {
        log_warning "Keyring installation encountered issues; check manually"
    }
    verify_setup || exit 1

    log_success "Latch setup complete on CLIENT!"
    log_info "You can now sync credentials to LXC containers."
    log_info "Example: make docker --with-secrets"
}

main
