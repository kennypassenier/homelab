#!/usr/bin/env bash
# Pre-sync hook for the vikunja stack.
# Runs inside the LXC as root before docker compose is applied.
#
# The official vikunja/vikunja image runs its process as uid=1000 inside the
# container. In an unprivileged LXC, uid=1000 inside the container maps to
# uid=101000 on the Proxmox host. bootstrap-lxc.sh initialises appdata dirs
# owned by root (uid=0 in the LXC), so we must fix ownership here to prevent
# "permission denied" errors on the mounted files volume.

set -euo pipefail

VIKUNJA_DATA="/appdata/vikunja/vikunja/config"

if [[ -d "$VIKUNJA_DATA" ]]; then
    chown -R 1000:1000 "$VIKUNJA_DATA"
fi
