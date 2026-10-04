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
# redesign-final (B, coordinator 2026-10-04: two Activity cases went red
# on redesign-371 unnoticed): a PLAIN commit runs the cases of the pages it
# changes too (HOMELAB_CASES_MODE=commit, .githooks/pre-commit), with a
# cheaper rule for a kit file most pages import (merge-cases.mjs --commit:
# the smoke pages, Stacks and the Inbox); a fast-forward runs the merge's
# full set from .githooks/post-merge (HOMELAB_CASES_RANGE=<from>..<to>).
#
# Called by .githooks/pre-commit and post-merge; usable alone.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
mode=${HOMELAB_CASES_MODE:-merge}
kind=MERGE
if [ "$mode" = commit ]; then
  kind=COMMIT
elif [ -z "${HOMELAB_CASES_RANGE:-}" ]; then
  # pre-merge-commit says so (a merge git commits by itself writes no
  # MERGE_HEAD); a conflicted merge's own commit has MERGE_HEAD.
  [ "${HOMELAB_MERGE_COMMIT:-}" = 1 ] \
    || git rev-parse -q --verify MERGE_HEAD >/dev/null || exit 0
fi

if [ -n "${HOMELAB_CASES_RANGE:-}" ]; then
  mapfile -t changed < <(git diff --name-only "$HOMELAB_CASES_RANGE" -- admin/web)
else
  mapfile -t changed < <(git diff --cached --name-only -- admin/web)
fi
if [ "${#changed[@]}" -eq 0 ]; then
  # redesign-final-48: a kp-themes bump alone still runs the contrast case.
  if [ -n "${HOMELAB_CASES_RANGE:-}" ]; then
    git diff -U0 "$HOMELAB_CASES_RANGE" -- Cargo.lock | grep -q '^[-+].*chassis-rs' || exit 0
  else
    git diff --cached -U0 -- Cargo.lock | grep -q '^[-+].*chassis-rs' || exit 0
  fi
fi

blocked() {
  echo "$kind BLOCKED — $1" >&2
  echo "Remedy: $2" >&2
  exit 1
}
command -v node >/dev/null 2>&1 \
  || blocked "node not found, so the cases this merge affects cannot be chosen." \
             "install node, or merge with --no-verify from a machine that cannot run them."

t0=$(date +%s)
flag=()
[ "$mode" = commit ] && flag=(--commit)
# redesign-final-48: the 22-theme contrast case runs when a stylesheet or the
# kp-themes pin (Cargo.lock's chassis-rs line) changed.
if [ -n "${HOMELAB_CASES_RANGE:-}" ]; then
  lockdiff=$(git diff -U0 "$HOMELAB_CASES_RANGE" -- Cargo.lock)
else
  lockdiff=$(git diff --cached -U0 -- Cargo.lock)
fi
grep -q '^[-+].*chassis-rs' <<<"$lockdiff" && flag+=(--kp)
plan=$(cd admin/web && node --import ./test/support/kp-register.mjs scripts/merge-cases.mjs "${flag[@]}" "${changed[@]}") \
  || blocked "the merge's affected cases could not be worked out (above)." \
             "fix admin/web/scripts/merge-cases.mjs or the file it names."
pattern=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pattern"])')
count=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["cases"]))')
pages=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(", ".join(json.load(sys.stdin)["pages"]) or "none")')
if [ -z "$pattern" ]; then
  echo "pre-${mode}: no whole-screen case opens a page this ${mode} changes (pages: $pages)"
  exit 0
fi
echo "pre-${mode}: $count whole-screen cases open the pages this ${mode} changes ($pages)"

log=$(mktemp)
# A port of its own (another run, a gate, may hold the default one).
if [ -z "${INVARIANTS_PORT:-}" ]; then
  INVARIANTS_PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  export INVARIANTS_PORT
fi
runner=(scripts/invariants-run.sh)
slot="$HOME/.cache/claude-build-scratch/e2e-slot.sh"
[ -x "$slot" ] && runner=("$slot" scripts/invariants-run.sh)
rc=0
# A hook runs with GIT_DIR / GIT_INDEX_FILE set for this repository: the
# run's own fixture repository (git init, git commit) must not see them,
# or its commit lands on the branch being committed (it did, in a proof).
env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
  INVARIANTS_ONLY="$pattern" "${runner[@]}" </dev/null >"$log" 2>&1 || rc=$?
took=$(( $(date +%s) - t0 ))
if [ "$rc" -ne 0 ]; then
  echo "$kind BLOCKED — whole-screen cases this ${mode} affects failed (${took} s):" >&2
  grep -E '^✖ ' "$log" | grep -v '^✖ failing tests' | sort -u | sed 's/^/  /' >&2 || tail -20 "$log" >&2
  echo "Remedy: fix them (log: $log), then ${mode} again." >&2
  exit 1
fi
echo "pre-${mode}: $count affected whole-screen cases pass (${took} s)"
rm -f "$log"
