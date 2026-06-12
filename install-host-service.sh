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
    "$REPO_ROOT/apps/HOST"
    "$REPO_ROOT/host-daemon/target/release/HOST"
)

for candidate in "${binary_candidates[@]}"; do
    if [[ -f "$candidate" ]]; then
        binary_path="$candidate"
        break
    fi
done

if [[ -z "$binary_path" ]]; then
    ui_error "HOST binary not found in repo."
    ui_info "Expected one of: apps/HOST or host-daemon/target/release/HOST"
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
