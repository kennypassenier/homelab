# Latch Clone Credential Sync Setup Guide

This guide walks through setting up and using the **Latch Clone** feature to migrate credentials from your CLIENT desktop to LXC containers securely.

## Prerequisites

- **CLIENT:** Linux desktop with `latch` CLI and an OS keyring installed
- **LXC:** Debian/Ubuntu or Alpine-based container with `latch` CLI and keyring support
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

Or integrate into your `scripts/host/bootstrap-lxc.sh`:
```bash
# Copy latch setup script into container
pct push $VMID /home/kenny/Projects/homelab/scripts/lxc/setup-latch.sh /tmp/
pct exec $VMID /bin/bash /tmp/setup-latch.sh
```

### 3. Set Up Credentials on CLIENT (Using Existing Infisical/Latch)

Configure your credentials using your current method (Infisical or native latch):

```bash
# Example: Store GitHub PAT
latch keyring set github.pat "ghp_xxxxxxxxxxxx"

# Example: Store project-specific key
latch keyring set my-app.key "secret-key-value"
```

This stores credentials in your OS keyring securely.

### 4. Sync to LXC Container

From CLIENT desktop, invoke the CLIENT app to sync:

```bash
# Via CLIENT TUI menu: "Secrets" → "Sync from Desktop"
# Or programmatically:
cd /home/kenny/Projects/homelab
cargo run --release --bin CLIENT -- --sync-credentials-to <lxc-container-ip>
```

The workflow is:
1. **CLIENT requests offer** from LXC (generate ephemeral key pair + 10-min TTL)
2. **CLIENT encrypts payload** with offer's public key (never exposes plaintext)
3. **LXC decrypts and applies** credentials to its keyring

### 5. Verify Credentials Synced

Inside LXC container:
```bash
# Check keyring
latch keyring list

# Example output:
# github.pat ............. ✓ set
# my-app.key ............. ✓ set
```

Or via CLIENT API:
```bash
curl http://lxc.local:8080/api/secrets/keyring
# Returns:
# {
#   "latch_available": true,
#   "keyring_available": true,
#   "message": "Ready for credential sync"
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

```bash
# CLIENT: Install pass + GPG
sudo apt-get install -y pass gnupg
pass init "Your Name <you@example.com>"

# LXC:
apt-get install -y pass gnupg
```

### "Offer expired"

Offers are valid for 10 minutes by default. If sync takes longer:
1. Increase `--ttl-minutes` when generating offer
2. Reduce network latency or timeouts
3. Retry (CLIENT will generate a new offer automatically)

### "Keyring initialization failed"

If using `pass`, GPG setup might fail in containerized environments:

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

## Coexistence with Infisical

**Latch Clone does NOT replace Infisical yet.** Current state:

- **Infisical:** Still used for app-level secret injection and backend storage
- **Latch:** Manages OS keyring for CLI tools, Git operations, and inter-container credential migration

### Gradual Migration Path

1. **Phase 1 (now):** Latch Clone syncs credentials to LXC; apps still use Infisical
2. **Phase 2:** LXC apps pull from latch keyring when available
3. **Phase 3:** Latch becomes primary; Infisical remains optional for centralized backends

You can test both side-by-side safely.

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
