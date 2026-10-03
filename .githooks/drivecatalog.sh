#!/usr/bin/env bash
# drive-reach review M1: the Live view control catalog at commit.
#
# admin/web/js/drivecatalog.json is generated from the page modules'
# declarations (scripts/drivecatalog.mjs) and compiled into the dashboard;
# the client checks every `homelab ui` step against it. A commit that moves
# admin/web/ regenerates it from this tree and stages the result (as the
# test plan and the runbook are), then runs its own test alone (well under a
# second): a stale catalog, a duplicate id, an undeclared mark or field, or
# a fuzzy match out of step with the client's fails the commit here, not at
# the release. gates.sh skips the node tests at commit; this one is cheap.
#
# Called by .githooks/pre-commit from the repository root; usable alone.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
web="$root/admin/web"
blocked() {
  echo "COMMIT BLOCKED — $1" >&2
  echo "Remedy: $2" >&2
  exit 1
}
command -v node >/dev/null 2>&1 \
  || blocked "node not found, so the Live view control catalog cannot be checked." \
             "install node, or commit with --no-verify from a machine that cannot run it."
[ -d "$web/node_modules" ] \
  || blocked "admin/web/node_modules is missing, so the control catalog cannot be built." \
             "run \`npm ci\` in admin/web once."
cd "$web"
if ! out=$(node --import ./test/support/kp-register.mjs scripts/drivecatalog.mjs 2>&1); then
  echo "$out" | tail -15 >&2
  blocked "the Live view control catalog does not build from the page modules (above)." \
          "fix the declaration it names (drivable.js declare/declareField)."
fi
cd "$root"
if ! git diff --quiet -- admin/web/js/drivecatalog.json; then
  git add -- admin/web/js/drivecatalog.json
  echo "pre-commit: regenerated and staged admin/web/js/drivecatalog.json"
fi
cd "$web"
t0=$(date +%s%N)
if ! out=$(node --import ./test/support/kp-register.mjs --test test/drivecatalog.test.js 2>&1); then
  echo "$out" | grep -E "✖|Error|^\s+[+-] " | head -30 >&2
  blocked "admin/web/test/drivecatalog.test.js failed (above)." \
          "fix what it names, then commit again."
fi
echo "pre-commit: drivecatalog.test.js passed in $(( ($(date +%s%N) - t0) / 1000000 )) ms"
