# Inject Infisical Machine Identity credentials as environment variables into the LXC container
if [[ -f "scripts/host/.env" ]]; then
    CLIENT_ID=$(grep '^INFISICAL_CLIENT_ID=' scripts/host/.env | cut -d= -f2-)
    CLIENT_SECRET=$(grep '^INFISICAL_CLIENT_SECRET=' scripts/host/.env | cut -d= -f2-)
    if [[ -n "$CLIENT_ID" && -n "$CLIENT_SECRET" ]]; then
        ui_step "Injecting INFISICAL_CLIENT_ID and INFISICAL_CLIENT_SECRET into LXC container environment..."
        pct set "$VMID" -ef INFISICAL_CLIENT_ID="$CLIENT_ID" -ef INFISICAL_CLIENT_SECRET="$CLIENT_SECRET"
        ui_success "INFISICAL_CLIENT_ID and INFISICAL_CLIENT_SECRET injected as environment variables."
    else
        ui_warning "INFISICAL_CLIENT_ID or INFISICAL_CLIENT_SECRET not found in scripts/host/.env; skipping environment injection."
    fi
fi
#!/usr/bin/env bash
# Script Name: bootstrap-lxc.sh
# Description: Bootstraps an LXC container interactively, configures fast local SSD storage, and sets up the environment.

set -euo pipefail

# Source the shared UI library
if [[ -f "scripts/shared/lib-ui.sh" ]]; then
    source "scripts/shared/lib-ui.sh"
else
    # Fallback functions if the library is not found
    ui_info() { echo "INFO: $1"; }
    ui_success() { echo "SUCCESS: $1"; }
    ui_warning() { echo "WARNING: $1"; }
    ui_error() { echo "ERROR: $1"; }
    ui_step() { echo "-> $1"; }
    ui_run_pacman() {
        local msg="$1"
        shift
        echo "Starting: $msg"
        "$@" > /dev/null 2>&1
        echo "Done: $msg"
    }
fi

# --- Rollback & Error Handling ---
cleanup_on_error() {
    local exit_code=$?
    # Only trigger rollback on actual errors
    if [[ $exit_code -ne 0 ]]; then
        echo ""
        ui_error "Bootstrap process failed unexpectedly! (Exit code: $exit_code)"
        ui_warning "Initiating safety rollback procedures..."

        # Stop the container safely to prevent undefined states
        if [[ -n "${VMID:-}" ]]; then
            ui_info "Stopping LXC container ${VMID}..."
            pct stop "${VMID}" 2>/dev/null || true
        fi

        echo ""
        ui_step "Troubleshooting tips:"
        ui_info "1. Verify your GITHUB_PAT is correct."
        ui_info "2. Ensure the LXC container has active internet access (Gateway/DNS)."
        ui_info "3. If the container or storage is corrupted, reset it and try again:"
        ui_info "   ./scripts/host/reset-stack.sh ${VMID:-<VMID>} ${STACK_NAME:-<STACK_NAME>}"
        echo ""
    fi
}
trap cleanup_on_error EXIT

# Safely load environment variables if present (e.g. GITHUB_USERNAME, GITHUB_PAT)
if [[ -f ".env" ]]; then
    chmod 600 .env
    set -a
    source .env
    set +a
elif [[ -f "scripts/host/.env" ]]; then
    chmod 600 "scripts/host/.env"
    set -a
    source "scripts/host/.env"
    set +a
fi

show_help() {
    echo "Usage: $0 [-v <VMID>] [-s <STACK_NAME>] [-u <GITHUB_USERNAME>] [-h]"
    echo ""
    echo "Options:"
    echo "  -v    Proxmox VMID"
    echo "  -s    Stack name"
    echo "  -u    GitHub Username"
    echo "  -h    Show this help message"
    echo ""
    echo "Secrets (GITHUB_PAT) must NOT be passed as CLI flags — they would"
    echo "be visible to all users via 'ps aux'. Provide them via a .env file."
    echo ""
    echo "If options are omitted, the script will run interactively and check for a .env file."
    exit 0
}

# Initialize variables from Environment (or empty)
VMID="${VMID:-}"
STACK_NAME="${STACK_NAME:-}"
GITHUB_USERNAME="${GITHUB_USERNAME:-}"
GITHUB_PAT="${GITHUB_PAT:-}"
 
# NOTE: GITHUB_PAT is intentionally NOT accepted as a CLI flag.
# Passing secrets via command-line arguments exposes them in 'ps aux', making them
# visible to any user on the host. Use a .env file instead.
while getopts "v:s:u:h" opt; do
    case "$opt" in
        v) VMID="$OPTARG" ;;
        s) STACK_NAME="$OPTARG" ;;
        u) GITHUB_USERNAME="$OPTARG" ;;
        h) show_help ;;
        *) show_help ;;
    esac
done

ui_info "=== LXC Bootstrap Wizard ==="
echo ""

# 1. Prompt for VMID
if [[ -z "$VMID" ]]; then
    read -r -p "Enter the VMID for the new LXC container: " VMID
fi

# 2. Prompt for Stack dynamically
if [[ -z "$STACK_NAME" ]]; then
    if [[ ! -d "stacks" ]]; then
        ui_error "Run this script from the root of the repository."
        exit 1
    fi

    ui_step "Available stacks:"
    stacks=()
    for dir in stacks/*/; do
        if [[ -d "$dir" ]]; then
            stacks+=("$(basename "$dir")")
        fi
    done

    if [[ ${#stacks[@]} -eq 0 ]]; then
        ui_error "No stacks found in stacks/ directory."
        exit 1
    fi

    for i in "${!stacks[@]}"; do
        echo "$((i+1)). ${stacks[$i]}"
    done

    while true; do
        read -r -p "Select a stack to deploy (1-${#stacks[@]}): " choice
        if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#stacks[@]}" ]; then
            STACK_NAME="${stacks[$((choice-1))]}"
            break
        else
            ui_warning "Invalid selection. Please enter a number between 1 and ${#stacks[@]}."
        fi
    done
fi

# 3. Prompt for GitHub Username
if [[ -z "$GITHUB_USERNAME" ]]; then
    read -r -p "Enter your GitHub username (or set GITHUB_USERNAME in .env): " GITHUB_USERNAME
fi

# 4. Validate secrets — these must come from .env, not interactive prompts.
# GITHUB_PAT as a CLI arg or prompt would expose it in 'ps aux'; .env is the
# only acceptable source on a shared Proxmox host.
if [[ -z "$GITHUB_PAT" ]]; then
    ui_error "GITHUB_PAT is not set. Add it to scripts/host/.env on the Proxmox host."
    ui_info "Example: echo 'GITHUB_PAT=ghp_...' >> scripts/host/.env && chmod 600 scripts/host/.env"
    exit 1
fi

GITOPS_DIR="/opt/gitops"

# Storage Automation: Fast Local NVMe Storage for App Configs/Databases
HOST_STORAGE_PATH="/opt/appdata/${STACK_NAME}"
LXC_MOUNT_POINT="/appdata"

echo ""
ui_step "Initiating bootstrap sequence for container ${VMID} targeting stack '${STACK_NAME}'..."

# Step 1: Ensure host directory exists and adjust permissions for unprivileged LXC
ui_step "Configuring isolated host SSD storage..."
bash -c "mkdir -p '${HOST_STORAGE_PATH}' && chown -R 100000:100000 '${HOST_STORAGE_PATH}'"
ui_success "Storage configured."


# Step 2: Automatically bind mount the host directory to the LXC container
ui_step "Bind mounting storage to LXC container..."
pct set "${VMID}" -mp0 "${HOST_STORAGE_PATH},mp=${LXC_MOUNT_POINT}"
ui_success "Storage mounted."



# Step 2a: Ensure /appdata and all /appdata/<stack>/<app> directories exist inside the container after it is started
# This must be done after pct start, as the mount is only available then


# Step 2b: Auto-detect if any compose file in this stack requires /dev/net/tun (e.g. gluetun).
# If so, configure TUN passthrough on the LXC *before* the container starts — the config
# only takes effect at boot time, so this must happen here rather than in a pre-sync hook.
CONF_FILE="/etc/pve/lxc/${VMID}.conf"
if grep -rl "/dev/net/tun" "stacks/${STACK_NAME}/" 2>/dev/null | grep -q .; then
    if grep -q "lxc.cgroup2.devices.allow: c 10:200" "${CONF_FILE}" 2>/dev/null; then
        ui_info "TUN passthrough already configured — skipping."
    else
        ui_step "Detected /dev/net/tun usage in stack '${STACK_NAME}' — enabling TUN passthrough..."
        if [[ ! -e "/dev/net/tun" ]]; then
            ui_error "/dev/net/tun does not exist on this host. Enable it with: modprobe tun"
            exit 1
        fi
        cat <<EOF >> "${CONF_FILE}"

# --- Added by bootstrap-lxc.sh (auto-detected TUN requirement) ---
# Allow the container's cgroup to access the TUN character device (major 10, minor 200).
# Required for VPN clients (e.g. gluetun) running inside Docker.
lxc.cgroup2.devices.allow: c 10:200 rwm

# Bind mount the host's TUN device node into the container so Docker can see it.
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
# ----------------------------------------------------------------
EOF
        ui_success "TUN passthrough configured."
    fi
fi


# Step 3: Start the container
ui_step "Starting LXC container ${VMID}..."
pct start "${VMID}" || true
sleep 5
ui_success "Container started."

# Step 3a: Ensure /appdata and all app subdirectories exist inside the container (robust, with retries)
ui_step "Ensuring /appdata and all app subdirectories exist inside the container..."
max_retries=10
retry_delay=2
success=0
for attempt in $(seq 1 $max_retries); do
    if pct exec "${VMID}" -- test -d /appdata; then
        pct exec "${VMID}" -- mkdir -p /appdata
        for app_dir in stacks/${STACK_NAME}/*/; do
            app_name=$(basename "$app_dir")
            # Skip non-directories (e.g. pre-sync.sh, stack-mounts.yml)
            if [[ -d "$app_dir" ]]; then
                pct exec "${VMID}" -- mkdir -p "/appdata/${STACK_NAME}/${app_name}"
            fi
        done
        success=1
        break
    else
        ui_warning "/appdata not available in container yet (attempt $attempt/$max_retries), retrying in ${retry_delay}s..."
        sleep $retry_delay
    fi
done
if [[ $success -eq 1 ]]; then
    ui_success "/appdata and all app subdirectories ensured."
else
    ui_error "/appdata mount did not become available in the container after $((max_retries * retry_delay)) seconds."
    exit 1
fi

ui_step "Installing dependencies (Docker, Infisical CLI, security updates)..."
pct exec "${VMID}" -- bash -c "
apt-get update && apt-get install -y curl git wget openssl jq unattended-upgrades
dpkg-reconfigure -f noninteractive unattended-upgrades
curl -fsSL https://get.docker.com | sh
# Install Infisical CLI (Debian/Ubuntu)
curl -1sLf 'https://artifacts-cli.infisical.com/setup.deb.sh' | bash
apt-get update && apt-get install -y infisical
"
ui_success "Dependencies installed."

ui_step "Injecting GitOps synchronization script..."
# Copy minimal sparse-setup.sh (no SOPS/Age, no $3) into the container
pct push "${VMID}" "scripts/host/sparse-setup.sh" "/root/sparse-setup.sh"

ui_step "Executing sparse checkout..."
pct exec "${VMID}" -- bash -c "chmod +x /root/sparse-setup.sh && /root/sparse-setup.sh ${STACK_NAME} ${GITHUB_PAT}"
ui_success "Sparse checkout complete."

# Step 6: Push GitHub Public Key for SSH access
ui_step "Fetching GitHub SSH key for authentication..."
pct exec "${VMID}" -- bash -c "
mkdir -p /root/.ssh && chmod 700 /root/.ssh
curl -sL https://github.com/${GITHUB_USERNAME}.keys >> /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys
"
ui_success "SSH key configured."

# Step 7: Configure automated GitOps synchronization (Cronjob + logrotate)
# Using >> (append) so successive sync runs accumulate in the log rather than each run
# wiping the previous output. Logrotate keeps 7 days of compressed history and then
# truncates the live file, so the log never grows unbounded.
ui_step "Configuring 5-minute GitOps reconciliation loop..."
pct exec "${VMID}" -- bash -c "
        echo '*/5 * * * * root ${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME} >> /var/log/node-sync.log 2>&1' > /etc/cron.d/gitops-sync
        cat > /etc/logrotate.d/node-sync <<'LOGROTATE'
/var/log/node-sync.log {
    daily
    rotate 7
    compress
    missingok
    notifempty
}
LOGROTATE
    "
ui_success "Cronjob configured."

# Step 8: Trigger the initial docker-compose up
# This step is a convenience — the 5-minute cronjob will handle it if this fails.
# We therefore don't abort the entire bootstrap on failure; instead we surface the
# real error output so it can be debugged without rolling back a successful install.
ui_step "Triggering initial application deployment..."
if ! pct exec "${VMID}" -- bash -c "${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME}"; then
    ui_warning "Initial sync run failed (exit code $?). The cronjob will retry in 5 minutes."
    ui_info "To debug, run: pct exec ${VMID} -- bash -c '${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME}'"
else
    ui_success "Initial application deployment complete."
fi

# Step 9: Cleanup temporary bootstrap artifacts
ui_step "Cleaning up temporary bootstrap artifacts..."
pct exec "${VMID}" -- bash -c "rm -f /root/sparse-setup.sh && rm -rf /tmp/age"
ui_success "Cleanup done."

echo ""
ui_success "=== Bootstrap Completed ==="
ui_info "Fetch the MAC address below to set a static IP in OPNsense:"
pct config "${VMID}" | grep net0
