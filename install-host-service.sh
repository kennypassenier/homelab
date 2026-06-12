#!/usr/bin/env bash
# Install or refresh the HOST systemd service from the cloned homelab repo.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
SERVICE_PATH="/etc/systemd/system/host-daemon.service"
ENV_FILE="$REPO_ROOT/config/.env"

source "$REPO_ROOT/scripts/shared/lib-ui.sh"

if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
    ui_error "Run this installer as root on the Proxmox host."
    exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
    ui_error "Missing $ENV_FILE"
    ui_info "Restore it first with Latch before installing the service."
    exit 1
fi

binary_path=""
binary_candidates=(
    "$REPO_ROOT/apps/HOST-linux-x86_64-unknown-linux-gnu"
    "$REPO_ROOT/apps/HOST"
    "$REPO_ROOT/host-daemon/target/release/HOST"
)

expected_arch="$(uname -m)"

validate_host_binary() {
    local candidate="$1"

    if [[ ! -f "$candidate" ]]; then
        echo "missing"
        return 1
    fi

    local size
    size="$(stat -c%s "$candidate" 2>/dev/null || echo 0)"
    if [[ "$size" -lt 1000000 ]]; then
        echo "too small (${size} bytes)"
        return 1
    fi

    local file_out
    file_out="$(file -b "$candidate" 2>/dev/null || true)"
    if [[ "$file_out" != *"ELF"* ]]; then
        echo "not an ELF binary (${file_out})"
        return 1
    fi

    case "$expected_arch" in
        x86_64)
            if [[ "$file_out" != *"x86-64"* ]]; then
                echo "arch mismatch for ${expected_arch} (${file_out})"
                return 1
            fi
            ;;
        aarch64|arm64)
            if [[ "$file_out" != *"ARM aarch64"* && "$file_out" != *"ARM64"* ]]; then
                echo "arch mismatch for ${expected_arch} (${file_out})"
                return 1
            fi
            ;;
    esac

    echo "ok"
    return 0
}

invalid_notes=()

for candidate in "${binary_candidates[@]}"; do
    if [[ ! -f "$candidate" ]]; then
        continue
    fi

    if note="$(validate_host_binary "$candidate")"; then
        binary_path="$candidate"
        break
    else
        invalid_notes+=("$candidate -> $note")
    fi
done

if [[ -z "$binary_path" ]]; then
    ui_error "No valid HOST binary found in repo."
    ui_info "Expected one of: apps/HOST-linux-x86_64-unknown-linux-gnu, apps/HOST, or host-daemon/target/release/HOST"
    if [[ ${#invalid_notes[@]} -gt 0 ]]; then
        ui_warning "Invalid binary candidates:"
        for note in "${invalid_notes[@]}"; do
            ui_warning "  - $note"
        done
    fi
    exit 1
fi

chmod +x "$binary_path"

ui_section "Installing host-daemon.service"
cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=Homelab HOST daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=$REPO_ROOT
Environment=GITOPS_REPO=$REPO_ROOT
Environment=HOST_ENV_FILE=$ENV_FILE
Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
EnvironmentFile=-/etc/environment
EnvironmentFile=-$ENV_FILE
ExecStart=$binary_path
Restart=always
RestartSec=5
# Disable the default burst limit so the service restarts unconditionally
# after updates or crashes without ever entering the 'failed' state.
StartLimitIntervalSec=0
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

ui_info "Reloading systemd"
systemctl daemon-reload

ui_info "Enabling service on boot"
systemctl enable host-daemon.service

ui_info "Restarting service"
systemctl restart host-daemon.service

ui_info "Current service status"
systemctl --no-pager --full status host-daemon.service || true

ui_success "HOST service installed"
ui_info "Logs: journalctl -u host-daemon.service -f"
ui_info "Restart: systemctl restart host-daemon.service"
ui_info "Stop: systemctl stop host-daemon.service"
