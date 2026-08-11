# Use Case: HOST Daemon Self-Update

**Tier:** HOST (Proxmox level)  
**Status:** Implemented

---

## Overview

The HOST daemon checks GitHub releases for a newer `HOST` binary, downloads the release asset, replaces its own executable atomically, and restarts the systemd service.

---

## Implemented Behavior

- Manual update check is available from the HOST TUI with `U`
- A background update worker runs on a 30-minute interval by default
- Failsafe recovery also triggers update checks when the client heartbeat is stale
- Release asset lookup defaults to `HOST`
- Version comparison prevents downgrade attempts
- The running binary is replaced atomically and `host-daemon.service` is restarted

---

## Files

- `host-daemon/src/self_update.rs`
- `host-daemon/src/failsafe.rs`
- `host-daemon/src/main.rs`
- `Makefile` (`release-host` target)

---

## Release Path

`make release-host` builds the HOST binary, bumps the patch version, tags the release, and publishes the asset to GitHub Releases.
