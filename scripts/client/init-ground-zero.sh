#!/usr/bin/env bash
# Script Name: init-ground-zero.sh
# Description: Initializes Age keys, encrypts the private key, and sets up Git filters
# so that.env files are automatically encrypted/decrypted seamlessly.

set -euo pipefail

mkdir -p secrets scripts/client scripts/host scripts/container stacks
echo "--- Initializing Homelab Ground Zero ---"

# Step 1: Install dependencies (Age via apt, SOPS via GitHub)
if ! command -v age-keygen &> /dev/null; then
    echo "Installing age..."
    sudo apt-get update && sudo apt-get install -y age
fi

if ! command -v sops &> /dev/null; then
    echo "Installing SOPS..."
    SOPS_VERSION="3.9.1"
    curl -sSL "https://github.com/getsops/sops/releases/download/v${SOPS_VERSION}/sops-v${SOPS_VERSION}.linux.amd64" -o /tmp/sops
    sudo install -m 755 /tmp/sops /usr/local/bin/sops
    rm /tmp/sops
fi

# Step 2: Generate Age Keypair
AGE_KEY_FILE="$HOME/.config/sops/age/keys.txt"
mkdir -p "$(dirname "$AGE_KEY_FILE")"

# Check if the key already exists before generating a new one
if [ ! -f "$AGE_KEY_FILE" ]; then
    age-keygen -o "$AGE_KEY_FILE"
else
    echo "Age key already exists at $AGE_KEY_FILE, skipping generation."
fi

PUBLIC_KEY=$(grep "public key:" "$AGE_KEY_FILE" | awk '{print $4}')

# Step 3: Create the.sops.yaml routing file
# We use '.*' here to avoid SOPS stdin filename matching errors during git clean filters
cat <<EOF >.sops.yaml
creation_rules:
  - path_regex: .*
    key_groups:
    - age:
      - $PUBLIC_KEY
EOF

# Step 4: Symmetrically encrypt the private key for the Git repository
read -s -p "Enter a strong passphrase to protect your Age key (AGE_PASSPHRASE): " AGE_PASSPHRASE
echo
# Pass the passphrase via file descriptor 3 instead of -pass pass:"..." to prevent it
# from being visible in 'ps aux' for the duration of the openssl process.
openssl enc -aes-256-cbc -pbkdf2 -salt -in "$AGE_KEY_FILE" -out secrets/age.key.enc -pass fd:3 3<<<"${AGE_PASSPHRASE}"

# Step 5: Configure Git smudge and clean filters for seamless workflow
git config --local filter.sops-env.clean "sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.smudge "sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.required true

echo "*.env filter=sops-env diff=sops-env" >.gitattributes

echo "Setup complete. The transparent Git encryption is now active."
echo "Please commit the generated files (.sops.yaml,.gitattributes, secrets/age.key.enc)."
