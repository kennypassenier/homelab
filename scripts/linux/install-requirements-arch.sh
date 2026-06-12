#!/usr/bin/env bash
# install-requirements-arch.sh
# Installs required tools for this repository on Arch-family distributions (Arch, Garuda, EndeavourOS).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LIB_UI="${REPO_ROOT}/scripts/shared/lib-ui.sh"
if [[ -f "$LIB_UI" ]]; then
    # shellcheck disable=SC1090
    source "$LIB_UI"
else
    ui_info() { echo "INFO: $*" >&2; }
    ui_success() { echo "SUCCESS: $*" >&2; }
    ui_warning() { echo "WARNING: $*" >&2; }
    ui_error() { echo "ERROR: $*" >&2; }
    ui_step() { echo "STEP: $*" >&2; }
    ui_header() { echo "$*" >&2; }
fi

print_help() {
    cat <<'EOF'
Usage: ./scripts/linux/install-requirements-arch.sh [--help]

Installs core dependencies for building and releasing CLIENT, HOST, and LXC.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    print_help
    exit 0
fi

if ! command -v sudo >/dev/null 2>&1; then
    ui_error "sudo is required"
    exit 1
fi

ui_header "Homelab Requirements Installer (Arch Family)"
ui_step "Updating package index and installing dependencies"

sudo pacman -Syu --needed --noconfirm \
    base-devel \
    git \
    curl \
    wget \
    make \
    jq \
    openssh \
    rsync \
    ca-certificates \
    gnupg \
    unzip \
    tar \
    gzip \
    xz \
    pkgconf \
    openssl \
    clang \
    cmake \
    protobuf \
    docker \
    docker-compose \
    github-cli \
    rustup \
    mingw-w64-gcc \
    pass \
    restic \
    rclone

ui_step "Enabling Docker daemon"
sudo systemctl enable --now docker

ui_step "Adding current user to docker group"
sudo usermod -aG docker "$USER" || true

if ! command -v rustup >/dev/null 2>&1; then
    ui_error "rustup was not installed correctly"
    exit 1
fi

ui_step "Installing stable Rust toolchain"
rustup default stable

ui_step "Adding Windows build target for CLIENT"
rustup target add x86_64-pc-windows-gnu

ui_success "Arch-family requirements installed"
ui_info "Open a new shell session before using docker without sudo"
