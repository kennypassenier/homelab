#!/usr/bin/env bash
# Script Name: proxmox-bootstrap-lxc.sh
# Description: Bootstraps an LXC container, configures fast local SSD storage, installs SOPS, and decrypts keys.
# Usage:./proxmox-bootstrap-lxc.sh <VMID> <STACK_NAME> <GITHUB_PAT> <AGE_PASSPHRASE> <GITHUB_USERNAME>

set -euo pipefail

if [[ $# -ne 5 ]]; then
    echo "Usage: $0 <VMID> <STACK_NAME> <GITHUB_PAT> <AGE_PASSPHRASE> <GITHUB_USERNAME>"
    exit 1
fi

VMID="$1"
STACK_NAME="$2"
GITHUB_PAT="$3"
AGE_PASSPHRASE="$4"
GITHUB_USERNAME="$5"
GITOPS_DIR="/opt/gitops"

# Storage Automation: Fast Local NVMe Storage for App Configs/Databases
HOST_STORAGE_PATH="/opt/appdata/${STACK_NAME}"
LXC_MOUNT_POINT="/appdata"

echo "Initiating bootstrap sequence for container ${VMID} targeting stack ${STACK_NAME}..."

# Step 1: Ensure host directory exists and adjust permissions for unprivileged LXC
mkdir -p "${HOST_STORAGE_PATH}"
chown -R 100000:100000 "${HOST_STORAGE_PATH}"

# Step 2: Automatically bind mount the host directory to the LXC container
pct set "${VMID}" -mp0 "${HOST_STORAGE_PATH},mp=${LXC_MOUNT_POINT}"

# Step 3: Start the container
pct start "${VMID}" || true
sleep 5 # Wait for network initialization

# Step 4: Install dependencies including Docker, Age, and SOPS
echo "Installing utilities and encryption tooling..."
pct exec "${VMID}" -- bash -c "
apt-get update && apt-get install -y curl git wget openssl jq
curl -fsSL https://get.docker.com | sh
wget -qO /usr/local/bin/sops https://github.com/getsops/sops/releases/download/v3.9.1/sops-v3.9.1.linux.amd64
chmod +x /usr/local/bin/sops
wget -qO- https://github.com/FiloSottile/age/releases/download/v1.1.1/age-v1.1.1-linux-amd64.tar.gz | tar -xzf - -C /tmp/
mv /tmp/age/age /tmp/age/age-keygen /usr/local/bin/
"

# Step 5: Inject synchronization script and setup transparent Git encryption
echo "Injecting synchronization script..."
pct exec "${VMID}" -- bash -c "cat > /root/sparse-setup.sh" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
REPO_URL="https://github.com/kennypassenier/homelab.git"
STACK_DIR="apps/$1"
AUTH_REPO_URL=$(echo "$REPO_URL" | sed "s|https://|https://$2@|g")

mkdir -p /opt/gitops
cd /opt/gitops || exit 1
git clone --no-checkout --filter=blob:none "$AUTH_REPO_URL".

# Decrypt the Age key to enable Git SOPS filter
mkdir -p /root/.config/sops/age
openssl enc -d -aes-256-cbc -pbkdf2 -salt -in secrets/age.key.enc -out /root/.config/sops/age/keys.txt -pass pass:"$3"
chmod 600 /root/.config/sops/age/keys.txt

# Setup transparent Git filters before checkout!
git config --local filter.sops-env.clean "sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.smudge "sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.required true

# Setup sparse checkout
git sparse-checkout init --cone
git sparse-checkout set "$STACK_DIR" "scripts/container" "secrets" ".sops.yaml"

# Checkout main (Smudge filter automatically decrypts the.env files here)
git checkout main
EOF

pct exec "${VMID}" -- bash -c "chmod +x /root/sparse-setup.sh && /root/sparse-setup.sh ${STACK_NAME} ${GITHUB_PAT} ${AGE_PASSPHRASE}"

# Step 6: Push GitHub Public Key for SSH access
echo "Fetching GitHub SSH key for authentication..."
pct exec "${VMID}" -- bash -c "
mkdir -p /root/.ssh && chmod 700 /root/.ssh
curl -sL https://github.com/${GITHUB_USERNAME}.keys >> /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys
"

# Step 7: Trigger the initial docker-compose up
pct exec "${VMID}" -- bash -c "${GITOPS_DIR}/scripts/container/node-sync.sh ${STACK_NAME}"

echo "Bootstrap completed. Fetch the MAC address for OPNsense:"
pct config "${VMID}" | grep net0
