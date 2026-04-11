#!/usr/bin/env bash
# Script Name: restore-client.sh
# Description: Restores the SOPS/Age setup on a new machine using the existing
#              encrypted key from the repository. Run this after cloning the repo
#              on a new desktop — it does NOT generate a new key.

set -euo pipefail

source "scripts/shared/lib-ui.sh"

# Ensure we are running from the root of the repo
if [[ ! -d "stacks" || ! -d "scripts" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

# Abort cleanly on Ctrl+C
trap 'echo ""; ui_info "Cancelled."; exit 0' INT

ui_section "Restore Client — SOPS/Age Setup"

ui_info "This script restores your encryption keys on a new machine."
ui_info "You will need the passphrase you set during the initial Ground Zero setup."
echo ""

# --- Step 1: Check encrypted key file exists ---
if [[ ! -f "secrets/age.key.enc" ]]; then
    ui_error "secrets/age.key.enc not found. Is this the correct repository?"
    exit 1
fi

# --- Step 2: Install dependencies ---
ui_step "Checking dependencies..."

if ! command -v age &>/dev/null; then
    ui_spin "Installing age..." sudo apt-get install -y age
fi

if ! command -v sops &>/dev/null; then
    SOPS_VERSION="3.9.1"
    ui_spin "Installing SOPS ${SOPS_VERSION}..." bash -c "
        curl -sSL 'https://github.com/getsops/sops/releases/download/v${SOPS_VERSION}/sops-v${SOPS_VERSION}.linux.amd64' -o /tmp/sops &&
        sudo install -m 755 /tmp/sops /usr/local/bin/sops &&
        rm /tmp/sops
    "
fi

ui_success "Dependencies OK."
echo ""

# --- Step 3: Decrypt the Age private key ---
AGE_KEY_DIR="$HOME/.config/sops/age"
AGE_KEY_FILE="${AGE_KEY_DIR}/keys.txt"

if [[ -f "$AGE_KEY_FILE" ]]; then
    ui_warning "An Age key already exists at ${AGE_KEY_FILE}."
    if ! ui_confirm "Overwrite it with the key from this repository?" "false"; then
        ui_info "Keeping existing key. Skipping decryption."
    else
        _do_decrypt=true
    fi
else
    _do_decrypt=true
fi

if [[ "${_do_decrypt:-false}" == "true" ]]; then
    mkdir -p "$AGE_KEY_DIR"
    chmod 700 "$AGE_KEY_DIR"

    ui_step "Enter your Age passphrase to decrypt the key..."
    echo ""

    # Read passphrase securely (no echo)
    local_passphrase=""
    read -r -s -p "${UI_INDENT}Passphrase: " local_passphrase
    echo ""

    # Decrypt using the same openssl settings used during init-ground-zero.sh
    if ! openssl enc -d -aes-256-cbc -pbkdf2 -in "secrets/age.key.enc" \
            -out "$AGE_KEY_FILE" -pass fd:3 3<<<"${local_passphrase}"; then
        ui_error "Decryption failed. Wrong passphrase?"
        rm -f "$AGE_KEY_FILE"
        exit 1
    fi

    chmod 600 "$AGE_KEY_FILE"
    unset local_passphrase
    ui_success "Age key restored to ${AGE_KEY_FILE}."
fi

echo ""

# --- Step 4: Configure Git filters ---
ui_step "Configuring Git smudge/clean filters..."

git config --local filter.sops-env.clean  "sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.smudge "sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin"
git config --local filter.sops-env.required true

ui_success "Git filters configured."
echo ""

# --- Step 5: Verify decryption works ---
ui_step "Verifying SOPS can decrypt .env files..."

# Find any encrypted .env file in the repo to test with
TEST_ENV=$(find stacks -name "*.env" | head -1 || true)

if [[ -n "$TEST_ENV" ]]; then
    if sops --decrypt --input-type dotenv --output-type dotenv "$TEST_ENV" > /dev/null 2>&1; then
        ui_success "Verification passed — SOPS can decrypt ${TEST_ENV}."
    else
        ui_warning "Verification failed on ${TEST_ENV}. The key may not match this repository."
        ui_info   "Try running 'git checkout -- .' to re-apply smudge filters, then test again."
    fi
else
    ui_info "No .env files found to verify against — skipping verification."
fi

echo ""
ui_success "Restore complete. You are fully operational on this machine."
ui_info   "Run 'git checkout -- .' if existing .env files appear encrypted in plain text."
