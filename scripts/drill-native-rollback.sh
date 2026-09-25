#!/usr/bin/env bash
# B7 · the native rollback drill, on real hardware.
#
# Proves the one promise every native service in this house leans on: an
# install whose binary does not come up is rolled back to the binary that
# was running, from OUTSIDE the app, and the service is still active
# afterwards. Nothing had rehearsed it since the mechanism was written.
#
# What it does, in order:
#   1. deploys stacks/drill — a throwaway native-only container (CT 119)
#      with one fake service, drillsvc, whose "binary" is a shell script;
#   2. installs a GOOD binary (sleeps forever) with `install-native --file`
#      and reads `systemctl is-active` on the container;
#   3. installs a BROKEN binary (exits at once) the same way and expects the
#      install to report a rollback;
#   4. measures on the container: the unit is active, and the binary's
#      sha256 is the GOOD one, not the broken one;
#   5. destroys the drill stack (no backup: it has no data by design).
#
# Every reading is taken from the container, never from the client's own
# claim of success — the drill exists because claims of success are the
# thing this project keeps finding to be wrong.
#
# Usage: scripts/drill-native-rollback.sh            (uses `homelab` on PATH)
#        HOMELAB="cargo run -q -p homelab-client --" scripts/drill-native-rollback.sh
set -euo pipefail
cd "$(dirname "$0")/.."
HOMELAB=${HOMELAB:-homelab}
PVE=${PVE:-pve}
VMID=119
UNIT=drillsvc
BIN=/opt/drillsvc/bin/drillsvc
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

printf '#!/bin/sh\n# drillsvc GOOD: stays up\nwhile :; do sleep 60; done\n' > "$work/good.sh"
printf '#!/bin/sh\n# drillsvc BROKEN: dies at once\necho "drillsvc: broken on purpose (B7 drill)" >&2\nexit 1\n' > "$work/broken.sh"
good_sha=$(sha256sum "$work/good.sh" | cut -d' ' -f1)
broken_sha=$(sha256sum "$work/broken.sh" | cut -d' ' -f1)

on_ct() { ssh -o ConnectTimeout=8 "$PVE" "pct exec $VMID -- sh -c '$*'"; }
say() { printf '\n\033[36m── %s\033[0m\n' "$*"; }
fail() { printf '\033[31mDRILL FAILED: %s\033[0m\n' "$*"; exit 1; }

say "1 · deploy the drill stack (CT $VMID)"
# The unit is written and enabled by the deploy; without a binary the unit
# cannot start, which the deploy reports. That is the expected starting
# point, not a fault — the binary arrives in step 2.
$HOMELAB deploy stacks/drill || echo "  (deploy reported the unit could not start yet — expected before the binary is installed)"

say "2 · install the GOOD binary"
$HOMELAB install-native stacks/drill/$UNIT --file "$work/good.sh"
sleep 3
state=$(on_ct "systemctl is-active $UNIT" || true)
sha=$(on_ct "sha256sum $BIN | cut -d\" \" -f1" || true)
echo "  container says: $UNIT is '$state', binary sha256 $sha"
[ "$state" = active ] || fail "the good binary did not come up"
[ "$sha" = "$good_sha" ] || fail "the binary on the container is not the good one"

say "3 · install the BROKEN binary and expect a rollback"
set +e
$HOMELAB install-native stacks/drill/$UNIT --file "$work/broken.sh" > "$work/broken.log" 2>&1
rc=$?
set -e
cat "$work/broken.log" | tail -8
[ "$rc" -ne 0 ] || fail "installing a broken binary reported success"
grep -q "rolled back" "$work/broken.log" || fail "the install did not say it rolled back"

say "4 · measure the container after the rollback"
sleep 3
state=$(on_ct "systemctl is-active $UNIT" || true)
sha=$(on_ct "sha256sum $BIN | cut -d\" \" -f1" || true)
restarts=$(on_ct "systemctl show $UNIT -p NRestarts --value" || true)
echo "  container says: $UNIT is '$state', binary sha256 $sha, NRestarts=$restarts"
[ "$state" = active ] || fail "the service is not active after the rollback"
[ "$sha" = "$good_sha" ] || fail "the binary after the rollback is not the good one ($sha vs good $good_sha, broken $broken_sha)"

say "5 · destroy the drill stack"
echo drill | $HOMELAB destroy stacks/drill --no-backup

printf '\n\033[32mDRILL PASSED\033[0m — a broken native release is rolled back and the service stays on the binary that worked (good %s, broken %s)\n' "${good_sha:0:12}" "${broken_sha:0:12}"
