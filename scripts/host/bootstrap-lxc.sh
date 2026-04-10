#!/usr/bin/env bash
# Script Name: bootstrap-lxc.sh
# Description: Bootstraps an LXC container interactively, configures fast local SSD storage, installs SOPS, and decrypts keys.

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

# Safely load environment variables if present (e.g. GITHUB_USERNAME, GITHUB_PAT, AGE_PASSPHRASE)
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
    echo "Usage: $0 [-v <VMID>] [-s <STACK_NAME>] [-u <GITHUB_USERNAME>] [-t <GITHUB_PAT>] [-a <AGE_PASSPHRASE>] [-h]"
    echo ""
    echo "Options:"
    echo "  -v    Proxmox VMID"
    echo "  -s    Stack name"
    echo "  -u    GitHub Username"
    echo "  -t    GitHub Personal Access Token"
    echo "  -a    Age key passphrase"
    echo "  -h    Show this help message"
    echo ""
    echo "If options are omitted, the script will run interactively and check for a .env file."
    exit 0
}

# Initialize variables from Environment (or empty)
VMID="${VMID:-}"
STACK_NAME="${STACK_NAME:-}"
GITHUB_USERNAME="${GITHUB_USERNAME:-}"
GITHUB_PAT="${GITHUB_PAT:-}"
AGE_PASSPHRASE="${AGE_PASSPHRASE:-}"

while getopts "v:s:u:t:a:h" opt; do
    case "$opt" in
        v) VMID="$OPTARG" ;;
        s) STACK_NAME="$OPTARG" ;;
        u) GITHUB_USERNAME="$OPTARG" ;;
        t) GITHUB_PAT="$OPTARG" ;;
        a) AGE_PASSPHRASE="$OPTARG" ;;
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
    if [[ ! -d "apps" ]]; then
        ui_error "Run this script from the root of the repository."
        exit 1
    fi

    ui_step "Available stacks:"
    stacks=()
    for dir in apps/*/; do
        if [[ -d "$dir" ]]; then
            stacks+=("$(basename "$dir")")
        fi
    done

    if [[ ${#stacks[@]} -eq 0 ]]; then
        ui_error "No stacks found in apps/ directory."
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

# 4. Prompt for Secrets securely
if [[ -z "$GITHUB_PAT" ]]; then
    read -s -p "Enter your GitHub Personal Access Token (GITHUB_PAT): " GITHUB_PAT
    echo ""
fi

if [[ -z "$AGE_PASSPHRASE" ]]; then
    read -s -p "Enter your Age key passphrase (AGE_PASSPHRASE): " AGE_PASSPHRASE
    echo ""
fi

GITOPS_DIR="/opt/gitops"

# Storage Automation: Fast Local NVMe Storage for App Configs/Databases
HOST_STORAGE_PATH="/opt/appdata/${STACK_NAME}"
LXC_MOUNT_POINT="/appdata"

echo ""
ui_step "Initiating bootstrap sequence for container ${VMID} targeting stack '${STACK_NAME}'..."

# Step 1: Ensure host directory exists and adjust permissions for unprivileged LXC
ui_run_pacman "Configuring isolated host SSD storage..." \
    bash -c "mkdir -p '${HOST_STORAGE_PATH}' && chown -R 100000:100000 '${HOST_STORAGE_PATH}'"

# Step 2: Automatically bind mount the host directory to the LXC container
ui_run_pacman "Bind mounting storage to LXC container..." \
    pct set "${VMID}" -mp0 "${HOST_STORAGE_PATH},mp=${LXC_MOUNT_POINT}"

# Step 3: Start the container
ui_run_pacman "Starting LXC container ${VMID}..." \
    bash -c "pct start '${VMID}' || true; sleep 5"

# Step 4: Install dependencies including Docker, Age, SOPS, and unattended-upgrades
ui_run_pacman "Installing dependencies (Docker, Age, SOPS, security updates)..." \
    pct exec "${VMID}" -- bash -c "
apt-get update && apt-get install -y curl git wget openssl jq unattended-upgrades
dpkg-reconfigure -f noninteractive unattended-upgrades
curl -fsSL https://get.docker.com | sh
wget -qO /usr/local/bin/sops https://github.com/getsops/sops/releases/download/v3.9.1/sops-v3.9.1.linux.amd64
chmod +x /usr/local/bin/sops
wget -qO- https://github.com/FiloSottile/age/releases/download/v1.1.1/age-v1.1.1-linux-amd64.tar.gz | tar -xzf - -C /tmp/
mv /tmp/age/age /tmp/age/age-keygen /usr/local/bin/
"

# Step 5: Inject synchronization script and setup transparent Git encryption
ui_run_pacman "Injecting GitOps synchronization script..." \
    pct exec "${VMID}" -- bash -c "cat > /root/sparse-setup.sh" << 'INNEREOF'
#!/usr/bin/env bash
set -euo pipefail
REPO_URL="https://github.com/kennypassenier/homelab.git"
STACK_DIR="apps/$1"
AUTH_REPO_URL=$(echo "$REPO_URL" | sed "s|https://|https://$2@|g")

mkdir -p /opt/gitops
cd /opt/gitops || exit 1
git clone --no-checkout --filter=blob:none "$AUTH_REPO_URL" .

# Extract and decrypt the Age key directly from the git tree without touching the working directory
mkdir -p /root/.config/sops/age
git show HEAD:secrets/age.key.enc | openssl enc -d -aes-256-cbc -pbkdf2 -salt -out /root/.config/sops/age/keys.txt -pass pass:"$3"
chmod 600 /root/.config/sops/age/keys.txt

# Setup transparent Git filters before checkout! Use absolute path to guarantee SOPS is found in LXC
git config --local filter.sops-env.clean "/usr/local/bin/sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.smudge "/usr/local/bin/sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.required true

# Setup sparse checkout safely
git sparse-checkout init --cone
git sparse-checkout set "$STACK_DIR" "scripts" "secrets" ".sops.yaml"

# Checkout main (Smudge filter automatically decrypts the .env files here)
git checkout main
INNEREOF

ui_run_pacman "Executing sparse checkout and decrypting secrets..." \
    pct exec "${VMID}" -- bash -c "chmod +x /root/sparse-setup.sh && /root/sparse-setup.sh ${STACK_NAME} ${GITHUB_PAT} ${AGE_PASSPHRASE}"

# Step 6: Push GitHub Public Key for SSH access
ui_run_pacman "Fetching GitHub SSH key for authentication..." \
    pct exec "${VMID}" -- bash -c "
mkdir -p /root/.ssh && chmod 700 /root/.ssh
curl -sL https://github.com/${GITHUB_USERNAME}.keys >> /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys
"

# Step 7: Configure automated GitOps synchronization (Cronjob)
ui_run_pacman "Configuring 5-minute GitOps reconciliation loop..." \
    pct exec "${VMID}" -- bash -c "echo '*/5 * * * * root ${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME} > /var/log/node-sync.log 2>&1' > /etc/cron.d/gitops-sync"

# Step 8: Trigger the initial docker-compose up
ui_run_pacman "Triggering initial application deployment..." \
    pct exec "${VMID}" -- bash -c "${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME}"

# Step 9: Cleanup temporary bootstrap artifacts
ui_run_pacman "Cleaning up temporary bootstrap artifacts..." \
    pct exec "${VMID}" -- bash -c "rm -f /root/sparse-setup.sh && rm -rf /tmp/age"

echo ""
ui_success "=== Bootstrap Completed ==="
ui_info "Fetch the MAC address below to set a static IP in OPNsense:"
pct config "${VMID}" | grep net0
