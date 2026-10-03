#!/usr/bin/env bash
# redesign-final-12 (coordinator, 2026-10-04): four whole-screen cases were
# red on redesign-371 although every branch had passed its own cases —
# nothing ran the cases a merge affects. On a merge commit (MERGE_HEAD
# exists: `git merge` runs pre-merge-commit, a conflicted merge's commit
# runs pre-commit; both reach here), the admin/web files the merge brings
# in are mapped to the whole-screen cases that open the pages built from
# them (admin/web/scripts/merge-cases.mjs, derived from main.js, the import
# graph and the cases' own addresses), and exactly those run against the
# demo host (scripts/invariants-run.sh, with its watchdog; through the
# shared e2e queue when this machine has one). Any failure refuses the
# merge and names the cases.
#
# Called by .githooks/pre-commit; usable alone on a merge in progress.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
# pre-merge-commit says so (a merge git commits by itself writes no
# MERGE_HEAD); a conflicted merge's own commit has MERGE_HEAD.
[ "${HOMELAB_MERGE_COMMIT:-}" = 1 ] \
  || git rev-parse -q --verify MERGE_HEAD >/dev/null || exit 0

mapfile -t changed < <(git diff --cached --name-only -- admin/web)
[ "${#changed[@]}" -gt 0 ] || exit 0

blocked() {
  echo "MERGE BLOCKED — $1" >&2
  echo "Remedy: $2" >&2
  exit 1
}
command -v node >/dev/null 2>&1 \
  || blocked "node not found, so the cases this merge affects cannot be chosen." \
             "install node, or merge with --no-verify from a machine that cannot run them."

t0=$(date +%s)
plan=$(cd admin/web && node --import ./test/support/kp-register.mjs scripts/merge-cases.mjs "${changed[@]}") \
  || blocked "the merge's affected cases could not be worked out (above)." \
             "fix admin/web/scripts/merge-cases.mjs or the file it names."
pattern=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pattern"])')
count=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["cases"]))')
pages=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(", ".join(json.load(sys.stdin)["pages"]) or "none")')
if [ -z "$pattern" ]; then
  echo "pre-merge: no whole-screen case opens a page this merge changes (pages: $pages)"
  exit 0
fi
echo "pre-merge: $count whole-screen cases open the pages this merge changes ($pages)"

log=$(mktemp)
runner=(scripts/invariants-run.sh)
slot="$HOME/.cache/claude-build-scratch/e2e-slot.sh"
[ -x "$slot" ] && runner=("$slot" scripts/invariants-run.sh)
rc=0
INVARIANTS_ONLY="$pattern" "${runner[@]}" </dev/null >"$log" 2>&1 || rc=$?
took=$(( $(date +%s) - t0 ))
if [ "$rc" -ne 0 ]; then
  echo "MERGE BLOCKED — whole-screen cases this merge affects failed (${took} s):" >&2
  grep -E '^✖ ' "$log" | grep -v '^✖ failing tests' | sort -u | sed 's/^/  /' >&2 || tail -20 "$log" >&2
  echo "Remedy: fix them on the merged tree (log: $log), or merge with --no-verify as a conscious act." >&2
  exit 1
fi
echo "pre-merge: $count affected whole-screen cases pass (${took} s)"
rm -f "$log"
