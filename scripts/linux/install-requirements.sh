#!/usr/bin/env bash
# install-requirements.sh
# Install dependencies for this repository on Linux by dispatching to a distro-specific script.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

print_help() {
    cat <<'EOF'
Usage: ./scripts/linux/install-requirements.sh [--arch|--debian] [--help]

Installs project requirements by detecting distro family automatically.

Options:
  --arch      Force Arch-family installer
  --debian    Force Debian-family installer
  -h, --help  Show this help
EOF
}

force_family=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --arch)
            force_family="arch"
            shift
            ;;
        --debian)
            force_family="debian"
            shift
            ;;
        -h|--help)
            print_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            print_help
            exit 1
            ;;
    esac
done

family="${force_family}"
if [[ -z "$family" ]]; then
    if command -v pacman >/dev/null 2>&1; then
        family="arch"
    elif command -v apt-get >/dev/null 2>&1; then
        family="debian"
    else
        echo "Unsupported distro: expected pacman or apt-get" >&2
        exit 1
    fi
fi

case "$family" in
    arch)
        exec "${SCRIPT_DIR}/install-requirements-arch.sh"
        ;;
    debian)
        exec "${SCRIPT_DIR}/install-requirements-debian.sh"
        ;;
    *)
        echo "Unsupported family: ${family}" >&2
        exit 1
        ;;
esac
