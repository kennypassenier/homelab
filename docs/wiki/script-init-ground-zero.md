# init-ground-zero.sh

> One-time bootstrap script that installs Age and SOPS, generates an encryption keypair, and configures Git smudge/clean filters for transparent `.env` encryption.

## Overview

`scripts/client/init-ground-zero.sh` sets up the [secret management](secret-management.md) infrastructure on a fresh machine. It should only be run **once** per repository lifetime. Running it again generates a new keypair, which makes all previously encrypted `.env` files permanently unreadable. The [client.sh](script-client-sh.md) menu guards against accidental re-runs with two red-warning confirmation prompts.

## Usage

```bash
./scripts/client/init-ground-zero.sh
# or (with guards) via:
./client.sh → SOPS/Age: First-Time Key Setup
```

Run from the repository root.

## What It Does (Step by Step)

### 1. Install Dependencies
- Installs `age` via `apt-get` if not present
- Downloads `sops` v3.9.1 binary from GitHub releases and installs to `/usr/local/bin/sops`

### 2. Generate Age Keypair
Runs `age-keygen -o ~/.config/sops/age/keys.txt`. Skips generation if the file already exists (the file check is separate from the guard in `client.sh` — running the script directly is still safe on a machine that already has a key).

### 3. Create `.sops.yaml`
Writes the SOPS routing file with the generated public key:
```yaml
creation_rules:
  - path_regex: .*
    key_groups:
    - age:
      - age1<your_public_key>
```

### 4. Encrypt the Private Key
Prompts for a passphrase (via `read -s` — no echo). Encrypts `~/.config/sops/age/keys.txt` with AES-256-CBC + PBKDF2 using `openssl`, writing to `secrets/age.key.enc`.

The passphrase is passed via file descriptor 3 (`-pass fd:3`) to prevent it from appearing in `ps aux`.

### 5. Configure Git Filters
Sets the smudge/clean filters in `.git/config` and writes `.gitattributes`:
```
*.env filter=sops-env diff=sops-env
```

## After Running

Commit the generated files:
```bash
git add .sops.yaml .gitattributes secrets/age.key.enc
git commit -m "feat(core): initialize SOPS/Age encryption"
git push
```

Keep the passphrase safe — it is the only way to recover the private key from `secrets/age.key.enc`.

## See also

- [script-restore-client.md](script-restore-client.md) — restoring the setup on a new machine
- [Secret Management](secret-management.md)
- [script-client-sh.md](script-client-sh.md)
