#!/usr/bin/env bash
# install-requirements-debian.sh
# Installs required tools for this repository on Debian/Ubuntu-family distributions.

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
Usage: ./scripts/linux/install-requirements-debian.sh [--help]

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

ui_header "Homelab Requirements Installer (Debian Family)"

ui_step "Installing base apt dependencies"
sudo apt-get update
sudo apt-get install -y \
    ca-certificates \
    curl \
    gnupg \
    lsb-release \
    software-properties-common \
    apt-transport-https \
    git \
    wget \
    make \
    jq \
    openssh-client \
    rsync \
    unzip \
    tar \
    gzip \
    xz-utils \
    pkg-config \
    libssl-dev \
    clang \
    cmake \
    protobuf-compiler \
    build-essential \
    gcc-mingw-w64-x86-64 \
    pass \
    restic \
    rclone

ui_step "Installing Docker Engine from official Docker apt repository"
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/$(. /etc/os-release && echo "$ID")/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/$(. /etc/os-release && echo "$ID") $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
    sudo tee /etc/apt/sources.list.d/docker.list >/dev/null

sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

ui_step "Installing GitHub CLI from official apt repository"
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | \
    sudo dd of=/etc/apt/keyrings/githubcli-archive-keyring.gpg
sudo chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | \
    sudo tee /etc/apt/sources.list.d/github-cli.list >/dev/null

sudo apt-get update
sudo apt-get install -y gh

if ! command -v rustup >/dev/null 2>&1; then
    ui_step "Installing rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

ui_step "Installing stable Rust toolchain"
rustup default stable

ui_step "Adding Windows build target for CLIENT"
rustup target add x86_64-pc-windows-gnu

ui_step "Enabling Docker daemon"
sudo systemctl enable --now docker

ui_step "Adding current user to docker group"
sudo usermod -aG docker "$USER" || true

ui_success "Debian-family requirements installed"
ui_info "Open a new shell session before using docker without sudo"
