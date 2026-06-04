# Use Case: CLIENT Build & Release Pipeline

**Tier:** CLIENT (local workstation)  
**Status:** Implemented

---

## Overview

The Makefile builds CLIENT binaries for Linux and Windows, auto-bumps the version, and publishes GitHub release assets for distribution.

---

## Implemented Behavior

- `make build-client` builds the Linux release binary
- `make build-client-windows` cross-compiles the Windows release binary
- `make release-client` bumps the patch version, builds both binaries, tags the release, and uploads both assets to GitHub Releases
- Release asset names follow the target-specific convention used by the repo

---

## Files

- `Makefile`
- `client-app/Cargo.toml`
- `scripts/shared/bump-patch-version.sh`

---

## Release Path

`make release-client` is the supported release entrypoint for CLIENT.
