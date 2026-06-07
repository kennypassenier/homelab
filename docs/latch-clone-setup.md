# Latch Clone Credential Sync Setup Guide

This guide walks through setting up and using the **Latch Clone** feature to migrate credentials from your CLIENT desktop to LXC containers securely.

## Prerequisites

- **CLIENT:** Linux desktop with `latch` CLI and an OS keyring installed
- **LXC:** Debian/Ubuntu or Alpine-based container with native `latch` CLI installed
- **Network:** CLIENT must be able to reach LXC daemon on `:8080` (typically via SSH tunnel or internal network)

## Quick Start

### 1. Install on CLIENT Desktop

```bash
# From homelab repository
./scripts/client/setup-latch.sh
```

This installs:
- **latch CLI** (credential manager)
- **pass** or other OS keyring (gnome-keyring, kwallet)
- **GPG** (for keyring encryption)

Verify:
```bash
./scripts/client/setup-latch.sh --verify-only
```

### 2. Install in LXC Container

During LXC bootstrap, run:
```bash
./scripts/lxc/setup-latch.sh
```

Default LXC mode is headless and env-backed:

- installs the latest `latch` GitHub release binary to `/usr/local/bin/latch`
- enables a guarded daily update timer
- relies on persistent `LATCH_PAT` / `LATCH_KEY` injected by HOST
- does not require `pass` or another keyring backend

If you explicitly want `pass` as an extra backend:

```bash
./scripts/lxc/setup-latch.sh --with-pass
```

Or integrate into your `scripts/host/bootstrap-lxc.sh`:
```bash
# Copy latch setup script into container
pct push $VMID /home/kenny/Projects/homelab/scripts/lxc/setup-latch.sh /tmp/
pct exec $VMID /bin/bash /tmp/setup-latch.sh
```

### 3. Set Up Credentials on CLIENT

Configure your credentials with the native latch flow:

```bash
latch login
latch init
```

Use `latch key --env prod` if you want a separate production-only key.

### 4. Sync to LXC Container

From CLIENT desktop, invoke the CLIENT app to sync:

```bash
# Via CLIENT TUI menu: "Secrets" → "Sync from Desktop"
# Or programmatically:
cd /home/kenny/Projects/homelab
cargo run --release --bin CLIENT -- --sync-credentials-to <lxc-container-ip>
```

The workflow is:
1. **Target LXC requests offer** with `latch clone offer`
2. **Source machine creates payload** with `latch clone create --offer-stdin`
3. **Target LXC applies payload** with `latch clone apply --stdin`

### 5. Verify Credentials Synced

Inside LXC container:
```bash
# Check sync status
latch status
```

Or via CLIENT API:
```bash
curl http://lxc.local:8080/api/secrets/keyring
# Returns:
# {
#   "latch_available": true,
#   "keyring_available": false,
#   "message": "Ready for headless latch operation via persistent LATCH_PAT/LATCH_KEY"
# }
```

## Advanced Usage

### Sync Only Specific Projects

```bash
# CLIENT TUI: Advanced options → "Filter by project"
# Or API call with project filters
```

### Integrity Verification

Add an optional one-time code to verify payload integrity:

```bash
CODE=$(openssl rand -hex 8)

# CLIENT side: (stored in .env or secure input)
# LXC side: (shared out-of-band)

# During sync: both offer and apply stages use --verify-code $CODE
```

### Piped Zero-Temp-Files Workflow

For agent automation (zero temp files, end-to-end encryption):

```bash
# On LXC:
latch clone offer --ttl-minutes 10 | \
  ssh client@source-host \
  'latch clone create --offer-stdin --verify-code shared-secret' | \
  latch clone apply --stdin --verify-code shared-secret
```

## Troubleshooting

### "latch CLI not found"

```bash
# CLIENT:
which latch || ./scripts/client/setup-latch.sh

# LXC:
pct exec $VMID which latch || pct exec $VMID /bin/bash ./scripts/lxc/setup-latch.sh
```

### "Keyring not available"

This is acceptable for normal LXC operation.
LXC containers default to persistent env-backed credentials via `LATCH_PAT` and `LATCH_KEY`.
Only install `pass` if you explicitly want an extra keyring backend.

```bash
# CLIENT: Install pass + GPG
sudo apt-get install -y pass gnupg
pass init "Your Name <you@example.com>"

# LXC (optional only):
apt-get install -y pass gnupg
```

### "Offer expired"

Offers are valid for 10 minutes by default. If sync takes longer:
1. Increase `--ttl-minutes` when generating offer
2. Reduce network latency or timeouts
3. Retry (CLIENT will generate a new offer automatically)

### "Keyring initialization failed"

If you explicitly chose `--with-pass`, GPG setup might fail in containerized environments:

```bash
# LXC: Manually init GPG
gpg --batch --gen-key << 'EOF'
Key-Type: RSA
Key-Length: 2048
Name-Real: LXC Daemon
Name-Email: lxc@homelab.local
Expire-Date: 0
%no-protection
EOF

# Then init pass
pass init "LXC Daemon <lxc@homelab.local>"
```

## Current Model

Latch is now the primary secret workflow in this repo.

- `latch commit` encrypts local `.env` files
- `latch push` uploads encrypted blobs to the secrets repo
- `latch pull --sparse` restores only stack-owned local `.env` files whose parent directories already exist
- `latch clone offer/create/apply` transfers credential state between machines

## Security Properties

✅ **End-to-end encryption:** x25519 ECDH between CLIENT and LXC  
✅ **No plaintext transit:** Payloads encrypted before leaving CLIENT  
✅ **Ephemeral keys:** New key pair per sync operation  
✅ **Offer expiry:** 10-minute TTL prevents replay attacks  
✅ **Optional integrity:** `--verify-code` adds HMAC tag  
✅ **Atomic apply:** Keyring restoration fails safely if validation fails  

## Environment Variables

Set in `~/.env` or CI/CD:

```bash
# LXC_API_BASE — URL to LXC daemon (default: http://lxc.local:8080)
export LXC_API_BASE="http://192.168.1.100:8080"

# LATCH_SYNC_TIMEOUT — max seconds to wait for each step (default: 30)
export LATCH_SYNC_TIMEOUT="60"

# LATCH_VERIFY_CODE — optional one-time code for integrity checking
export LATCH_VERIFY_CODE="$(openssl rand -hex 8)"
```

## Next Steps

- [ ] Test credential sync with a non-critical LXC container
- [ ] Integrate `latch clone` into CLIENT TUI (Secrets menu)
- [ ] Add keyring status monitoring to CLIENT dashboard
- [ ] Plan Phase 2: LXC apps pull from keyring
- [ ] Document app-level integration patterns
