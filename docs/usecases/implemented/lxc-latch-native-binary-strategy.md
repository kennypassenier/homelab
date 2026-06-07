# Use Case: Native Latch Binary Strategy for LXCs

**Tier:** HOST + LXC
**Status:** Implemented

## Goal

Use the native `latch` release binary inside LXCs instead of the Docker wrapper, while keeping secret operations non-interactive, persistent, and safe for sparse GitOps stacks.

## Problem

The previous LXC strategy wrapped `latch` in a Docker container.
That had three problems:

1. It added Docker as a dependency for secret management itself.
2. It blocked direct use of newer `latch` features like `latch pull --sparse` and native binary update flows.
3. It pushed LXC operators toward keyring/pass setup even though headless env-backed credentials are already sufficient for containerized operation.

## Implemented Strategy

LXCs now use this model:

1. HOST bootstrap injects persistent `LATCH_PAT` and `LATCH_KEY` into the container.
2. HOST bootstrap installs the latest native `latch` GitHub release binary via `scripts/lxc/setup-latch.sh`.
3. A guarded systemd timer in the LXC re-checks for new `latch` releases on a daily cadence.
4. Secret sync hooks should prefer `latch pull --sparse` so only stack-owned directories receive `.env` files.
5. `pass` or another keyring backend is optional in LXCs, not required.

## Why This Design

### Why native binary over Docker wrapper

- Direct access to current CLI features (`--sparse`, native update behavior).
- Lower runtime complexity.
- No need to spawn a container just to decrypt `.env` files.
- Better fit for pre-sync hooks and headless command execution.

### Why not require `pass` in LXCs

- Headless containers already have a stable credential source via environment injection.
- `latch-rs` supports fallback from OS keyring to `LATCH_PAT` / `LATCH_KEY` and then `~/.latch/config.toml`.
- Installing `pass` everywhere adds GPG lifecycle and extra failure modes with little operational benefit for non-interactive LXCs.

### Why guarded updates instead of "update on every contact"

- Avoids noisy GitHub polling and repeated update attempts.
- Decouples normal CLIENT/LXC contact from package lifecycle.
- Keeps update behavior predictable and rate-limited.

## Current Runtime Contract

### Persistent credentials

HOST pushes all `LATCH_*` variables from the host env into the LXC:

- `/root/.env`
- `/etc/environment`

That makes credentials persistent across restarts and available for non-interactive commands.

### Binary install/update knobs

These env keys control the native binary strategy:

```dotenv
LATCH_UPDATE_REPO=kennypassenier/latch-rs
LATCH_UPDATE_ASSET=latch-linux-x86_64.tar.gz
LATCH_UPDATE_INTERVAL_SECS=86400
```

### LXC setup behavior

`scripts/lxc/setup-latch.sh` now:

1. installs release prerequisites (`curl`, `jq`, `tar`, `ca-certificates`)
2. downloads the latest `latch` release asset from GitHub
3. installs `latch` to `/usr/local/bin/latch`
4. installs `/usr/local/bin/install-latch-release`
5. installs `/usr/local/bin/latch-update-safe`
6. enables `latch-update.timer`

## Operational Guidance

### Recommended stack hook behavior

When a stack uses Latch to materialize secret files, prefer:

```bash
latch pull --sparse --env prod
```

This preserves stack isolation because only already-existing parent directories receive `.env` files.

### Optional keyring backend

If a specific LXC needs `pass`, install it explicitly:

```bash
./scripts/lxc/setup-latch.sh --with-pass
```

That is opt-in, not the default path.

## Files

- `host-daemon/src/bootstrap.rs`
- `scripts/lxc/setup-latch.sh`
- `scripts/client/lib/lib-stack.sh`
- `lxc-daemon/src/api.rs`
- `config/.env`
- `config/.env.example`
- `docs/deployment.md`
- `docs/latch-clone-setup.md`
- `docs/lxc-features.md`

## Future Direction

APT-based installation remains the preferred long-term distribution model once `latch-rs` publishes a signed repository.
At that point, unattended-upgrades can replace the GitHub-release polling strategy for LXCs.
