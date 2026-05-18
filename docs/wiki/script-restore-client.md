# restore-client.sh

> Restores the SOPS/Age encryption setup on a new machine by decrypting the committed `secrets/age.key.enc` and reconfiguring Git filters.

## Overview

`scripts/client/restore-client.sh` is the counterpart to [init-ground-zero.sh](script-init-ground-zero.md). Run it after cloning the repository on a new Linux desktop. It does **not** generate a new key — it restores the existing one using the passphrase set during initial setup.

## Usage

```bash
./scripts/client/restore-client.sh
# or via menu:
./client.sh → SOPS/Age: Restore on New Machine
```

Run from the repository root.

## What It Does (Step by Step)

### 1. Check Prerequisites
Verifies `secrets/age.key.enc` exists. If not, the repository has not been initialised with [init-ground-zero.sh](script-init-ground-zero.md) yet.

### 2. Install Dependencies
Installs `age` via `apt-get` and `sops` v3.9.1 from GitHub if not present. Uses `ui_spin` for visual feedback.

### 3. Decrypt Age Private Key
Prompts for the passphrase (via `read -r -s` — no echo, passphrase never leaves the process via `fd:3`). Decrypts `secrets/age.key.enc` to `~/.config/sops/age/keys.txt` using the same `openssl enc -d -aes-256-cbc -pbkdf2` command used during creation.

If the passphrase is wrong, `openssl` returns non-zero and the script exits with an error, deleting the partially written key file.

If a key already exists at `~/.config/sops/age/keys.txt`, the user is asked whether to overwrite it (defaults to No).

### 4. Configure Git Filters
Sets the smudge/clean filters in `.git/config`:
```ini
[filter "sops-env"]
    clean  = sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin
    smudge = sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin
    required = true
```

### 5. Verify Decryption
Finds the first `.env` file in `stacks/` and attempts to decrypt it with SOPS. Prints a success or warning without failing — the filters may need a `git checkout -- .` on first use to re-apply the smudge to tracked files.

## After Running

```bash
# Re-apply smudge filters to get plaintext .env files in your working directory:
git checkout -- .
```

## See also

- [script-init-ground-zero.md](script-init-ground-zero.md)
- [Secret Management](secret-management.md)
- [script-client-sh.md](script-client-sh.md)
