# Use Case: LXC Daemon Docker Container Release

**Tier:** LXC (container runtime)  
**Status:** Implemented

---

## Overview

The LXC daemon is built and published as a Docker image to GHCR, and HOST bootstrap uses that image first when installing the daemon inside a new LXC.

---

## Implemented Behavior

- `make release-lxc` bumps the patch version, builds the LXC binary, builds the Docker image, and pushes both release artifacts
- The image is published to `ghcr.io/kennypassenier/homelab-lxc-daemon`
- `host-daemon/src/bootstrap.rs::install_lxc_daemon()` prefers the Docker image when `LXC_DAEMON_IMAGE` is configured
- If the image path is unavailable, HOST falls back to the built binary or a placeholder for development-only setups

---

## Files

- `lxc-daemon/Dockerfile`
- `host-daemon/src/bootstrap.rs`
- `Makefile`
- `.github/workflows/lxc-image.yml`

---

## Release Path

`make release-lxc` is the supported release entrypoint for the LXC daemon and Docker image.
