#!/bin/sh
# Runs when homelab-host enters the failed state (systemd OnFailure).
# Two jobs, both outside the daemon (it is dead at this point):
#  1. If a self-update marker is present: the new binary never came up
#     healthy - restore the previous binary and restart (H5 rollback).
#  2. Notify Home Assistant either way (F3) - the daemon cannot report
#     its own death.

WEBHOOK=$(grep "^notify_webhook" /etc/homelab/host.toml 2>/dev/null | cut -d\" -f2)

notify() {
  # $1 = op, $2 = error text
  [ -n "$WEBHOOK" ] || return 0
  curl -m 5 -s -o /dev/null -X POST -H "Content-Type: application/json" \
    -d "{\"source\":\"homelab-host\",\"op\":\"$1\",\"label\":\"systemd\",\"ok\":false,\"error\":\"$2\"}" \
    "$WEBHOOK" || true
}

if [ -f /var/lib/homelab/selfupdate.pending ]; then
  logger -t homelab-rollback "self-update failed - restoring previous binary"
  cp -a /usr/local/bin/homelab-host.prev /usr/local/bin/homelab-host
  rm -f /var/lib/homelab/selfupdate.pending
  notify "self-update-rollback" "new binary never came up healthy - previous binary restored and restarted"
  systemctl reset-failed homelab-host
  systemctl restart homelab-host
else
  logger -t homelab-rollback "daemon entered failed state (no self-update pending)"
  notify "daemon-failed" "homelab-host crashed repeatedly and systemd gave up - manual intervention needed (journalctl -u homelab-host)"
fi
