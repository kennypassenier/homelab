#!/usr/bin/env bash
# sweep-changed.sh — the commit-time Live view sweep (redesign-final,
# coordinator 2026-10-04): press only the controls this tree changed
# against HEAD (added, moved, given a `was`). A pass writes a partial
# admin/web/test-e2e/sweep-stamp.json naming them, which the commit guard
# (admin/web/test/drivecatalog.test.js) accepts for exactly those. No
# changed control: no sweep. The release gate still runs the full sweep
# and demands a full stamp (.githooks/gate-carry.sh invariants).
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
cd "$root/admin/web"
# The dashboard serves the catalog it was built with: regenerate it from the
# page modules first (as the commit hook does), so the sweep presses the
# entries this tree declares.
node --import ./test/support/kp-register.mjs scripts/drivecatalog.mjs >/dev/null
only=$(node --import ./test/support/kp-register.mjs scripts/sweep-changed.mjs)
if [ -z "$only" ]; then
  echo "sweep-changed: no Live view control changed against HEAD; no sweep needed"
  exit 0
fi
cd "$root"
INVARIANTS_SWEEP_ONLY="$only" \
INVARIANTS_ONLY="drive-reach: Live view finds and presses" \
  "$root/scripts/invariants-run.sh"
