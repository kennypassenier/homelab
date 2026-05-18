# Secret Management

> SOPS + Age provides transparent, Git-safe encryption of `.env` files — secrets are automatically encrypted on commit and decrypted on checkout.

## Overview

Secrets (API keys, passwords, tokens) are stored in `.env` files alongside the `docker-compose.yml` they belong to. These files are automatically encrypted before Git ever sees them and decrypted after Git checks them out — the workflow is completely transparent once configured. Nobody needs to remember to encrypt before pushing.

## How It Works

### The Tools

| Tool | Role |
|---|---|
| [Age](https://age-encryption.org/) | Asymmetric encryption. Generates a keypair (public + private). |
| [SOPS](https://github.com/getsops/sops) | Encrypts/decrypts structured files using Age keys. |
| Git smudge/clean filters | Hook into `git add` (clean = encrypt) and `git checkout` (smudge = decrypt). |

### The Flow

```
Working directory          Git index / remote
─────────────────          ──────────────────
.env (plaintext)  ──git add──▶  .env (SOPS-encrypted)
.env (plaintext)  ◀─checkout──  .env (SOPS-encrypted)
```

The filter configuration in `.gitattributes`:
```
*.env filter=sops-env diff=sops-env
```

And in `.git/config` (set by [init-ground-zero.sh](script-init-ground-zero.md)):
```ini
[filter "sops-env"]
    clean  = sops --encrypt --input-type dotenv --output-type dotenv /dev/stdin
    smudge = sops --decrypt --input-type dotenv --output-type dotenv /dev/stdin
    required = true
```

### Key Storage

- **Private key**: `~/.config/sops/age/keys.txt` on the developer's machine (never committed)
- **Encrypted backup**: `secrets/age.key.enc` in the repository — AES-256-CBC encrypted with a passphrase using `openssl`
- **Public key**: stored in `.sops.yaml` at the repo root, referenced during encryption

## First-Time Setup

Run once on a new machine that will manage secrets:

```bash
./client.sh → SOPS/Age: First-Time Key Setup
# or directly:
./scripts/client/init-ground-zero.sh
```

This will:
1. Install `age` and `sops` if missing
2. Generate a new Age keypair at `~/.config/sops/age/keys.txt`
3. Create `.sops.yaml` with your public key
4. Encrypt the private key with a passphrase → `secrets/age.key.enc`
5. Configure the Git smudge/clean filters

Commit `.sops.yaml`, `.gitattributes`, and `secrets/age.key.enc` to Git. **Never commit** `~/.config/sops/age/keys.txt`.

See [script-init-ground-zero.md](script-init-ground-zero.md) for full details.

## Restoring on a New Machine

After cloning the repo on a new desktop, run:

```bash
./client.sh → SOPS/Age: Restore on New Machine
# or directly:
./scripts/client/restore-client.sh
```

This decrypts `secrets/age.key.enc` using your passphrase and reinstalls the Age private key and Git filters. See [script-restore-client.md](script-restore-client.md).

## Inside LXC Containers

The [bootstrap-lxc.sh](script-bootstrap-lxc.md) script:
1. Installs SOPS inside the LXC
2. Decrypts `secrets/age.key.enc` using the `AGE_PASSPHRASE` (provided via `.env` or interactively)
3. Writes the Age private key to `~/.config/sops/age/keys.txt` inside the LXC
4. Configures the same Git smudge/clean filters in the LXC's Git config

After bootstrap, [node-sync.sh](script-node-sync.md) can `git pull` and the smudge filter automatically decrypts `.env` files so Docker Compose can read them.

## Working with `.env` Files

```bash
# Add a new .env value — just edit the file normally:
echo "MY_SECRET=hunter2" >> stacks/media/radarr/.env

# Stage it — SOPS clean filter encrypts it automatically:
git add stacks/media/radarr/.env

# The committed version is encrypted. Verify:
git show HEAD:stacks/media/radarr/.env  # shows SOPS ciphertext

# In your working directory it is still plaintext — the smudge filter kept it decrypted.
cat stacks/media/radarr/.env            # shows plaintext
```

## The `.sops.yaml` Routing File

```yaml
creation_rules:
  - path_regex: .*
    key_groups:
    - age:
      - age1...  # your public key
```

The `.*` regex matches all files (not just `.env`) to avoid stdin filename matching errors in the Git clean filter. In practice, only `.env` files are filtered via `.gitattributes`.

## Security Notes

- **Never pass `AGE_PASSPHRASE` or `GITHUB_PAT` as CLI arguments** — they would be visible in `ps aux`. Always use a `.env` file on the host or interactive prompts.
- `.env` files in `scripts/host/` are not committed and should be `chmod 600` on the Proxmox host.
- The encrypted `secrets/age.key.enc` is useless without the passphrase — it is safe to commit.

## See also

- [script-init-ground-zero.md](script-init-ground-zero.md)
- [script-restore-client.md](script-restore-client.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [GitOps Flow](gitops-flow.md)
- [Architecture Overview](architecture-overview.md)
