#!/usr/bin/env bash
# gate-carry.sh — rerun only what moved, after a recorded full run (fix-187).
#
# Kenny, 2026-10-02: "what is the difference between the gate and the
# release tests? are we doing double work?" Measured yes: on 2026-10-01 the
# full suite (cargo test --workspace --no-fail-fast + the admin web node
# tests) ran four times for one release, because every fix commit after a
# red `make gate` changes the tree, the gate-pass stamp is gone, and
# `make release` runs the whole thing again (standing rule 7: "tests run
# once per release, then only the failures are rerun" — this is the part
# that was not built yet).
#
# This script is called from .claude/hooks/gates.sh ONLY on the GATE_FULL
# path (`make gate` / `make release`'s own gate run); the commit-time
# subset (.githooks/test-subset.sh) is untouched. It decides, independently
# for the Rust suite and the admin/web node tests, whether to run the WHOLE
# thing again (full) or only a subset (carry) — and always says which, and
# why, in its own stdout.
#
# State lives under .git/, never in the tree, one subdirectory per family:
#
#   .git/gate-carry/rust/base-tree    tree the last run (full OR carry) saw
#   .git/gate-carry/rust/toolchain    rustc -V at that run (mismatch -> full)
#   .git/gate-carry/rust/failing.tsv  <package>\t<test-name>, one per line
#   .git/gate-carry/rust/last-run.txt human-readable note for `make release`
#   .git/gate-carry/node/*            the same four, for the node family
#
# A run — full or carry, green or red — always updates its family's state:
# the base-tree marks "the tree we last definitely looked at", not "the
# tree that passed"; a red run still narrows tomorrow's diff. Only a GREEN
# run may leave gate-pass stamped (that remains .claude/hooks/gates.sh's own
# call to .githooks/gate-stamp, unchanged by this file).
#
# Subcommands:
#   gate-carry.sh rust     decide + run the Rust suite (full or carry)
#   gate-carry.sh node     decide + run the admin/web node tests
#   gate-carry.sh full     force both to run in full (used by `make gate-full`)
set -uo pipefail
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root"
meta="$root/.githooks/gate-carry-meta.py"
# The state lives in the git dir, which in a linked worktree is a file
# pointer, not `$root/.git/`: ask git for the real (shared) directory.
gitdir="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)"

# GATE_CARRY_MODE=full is the escape hatch (`make gate-full`): skip the
# decision and run everything, same as before this file existed.
force_full="${GATE_CARRY_MODE:-}"

# ── shared helpers ──────────────────────────────────────────────────────

# Workspace-relevant files whose change can never be judged by a crate diff
# alone: the lockfile (any crate's resolved version moved), the toolchain
# pin, the workspace manifest, and the gate's own definition (editing the
# gate is exactly the case a stale carry must not paper over).
gate_global_trigger() {
  local changed="$1"
  grep -qxE 'Cargo\.lock|Cargo\.toml|rust-toolchain\.toml|Makefile' <<<"$changed" && { echo "Cargo.lock, the workspace manifest, the toolchain pin or the Makefile changed"; return 0; }
  grep -qE '^\.githooks/' <<<"$changed" && { echo "a file under .githooks/ changed (the gate's own machinery)"; return 0; }
  grep -qxF '.claude/hooks/gates.sh' <<<"$changed" && { echo ".claude/hooks/gates.sh changed (the gate's own definition)"; return 0; }
  return 1
}

# Prints the base tree to stdout, or nothing (and a reason to stderr) if
# there is none usable yet.
carry_base() {
  local family="$1" state="$gitdir/gate-carry/$1" cur_tc="$2"
  [ -s "$state/base-tree" ] || { echo "no earlier $family run recorded yet" >&2; return 1; }
  [ "$(cat "$state/toolchain" 2>/dev/null)" = "$cur_tc" ] || { echo "the toolchain changed since the last $family run" >&2; return 1; }
  local base
  base=$(cat "$state/base-tree")
  git cat-file -e "${base}^{tree}" 2>/dev/null || { echo "the last $family run's base tree is gone" >&2; return 1; }
  printf '%s' "$base"
}

# The tree gate-carry actually tested: `make gate` runs cargo against the
# WORKING tree, not the index, so `git write-tree` (which reads the index)
# would misrecord an unstaged edit as untested. `git stash create` builds a
# commit object for exactly the working tree + index, without touching the
# stash list or anything else; on a clean tree it prints nothing, and HEAD's
# own tree is already the right answer.
current_tree() {
  local sha
  sha=$(git stash create 2>/dev/null)
  if [ -n "$sha" ]; then
    git rev-parse "${sha}^{tree}"
  else
    git rev-parse 'HEAD^{tree}'
  fi
}

record_state() {  # record_state FAMILY TREE TOOLCHAIN FAILFILE KIND
  local family="$1" tree="$2" tc="$3" failsrc="$4" kind="$5"
  local state="$gitdir/gate-carry/$family"
  mkdir -p "$state"
  printf '%s' "$tree" > "$state/base-tree"
  printf '%s' "$tc" > "$state/toolchain"
  sort -u -- "$failsrc" > "$state/failing.tsv" 2>/dev/null || : > "$state/failing.tsv"
  printf '%s\n' "$kind" > "$state/kind"
}

# ── Rust family ─────────────────────────────────────────────────────────

# Parses a `cargo test --no-fail-fast` log into <package>\t<test-name> rows
# for every FAILED test. Cargo prints each binary's own path (relative to
# ITS crate, not the workspace), so the binary is identified by the stem in
# "Running ... (target/.../deps/<stem>-<hash>)" instead, looked up against
# gate-carry-meta.py's stem -> package table.
rust_parse_failures() {
  local log="$1" stemmap="$2"
  awk -F'\t' '{print $1"\t"$2}' "$stemmap" > /dev/null 2>&1 # just validate it is readable
  awk -v stemmap="$stemmap" '
    BEGIN {
      while ((getline line < stemmap) > 0) {
        split(line, a, "\t")
        pkg[a[1]] = a[2]
      }
    }
    /^     Running / {
      cur = $0
      sub(/^.*\/deps\//, "", cur)
      sub(/-[0-9a-f]{7,}[^-\/]*\)?[[:space:]]*$/, "", cur)
      stem = cur
      cur_pkg = (stem in pkg) ? pkg[stem] : stem
      next
    }
    /^test .* \.\.\. FAILED$/ {
      name = $0
      sub(/^test /, "", name)
      sub(/ \.\.\. FAILED$/, "", name)
      print cur_pkg "\t" name
    }
  ' "$log"
}

rust_run_full() {
  local log="$1"
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
      -u HOMELAB_TOKEN -u HOMELAB_HOST -u HOMELAB_CONFIG -u HOMELAB_LISTEN \
      cargo test --workspace --no-fail-fast 2>&1 | tee "$log"
  return "${PIPESTATUS[0]}"
}

rust_run_pkgs_full() {  # rust_run_pkgs_full LOG PKG...
  local log="$1"; shift
  [ $# -eq 0 ] && { : > "$log"; return 0; }
  local args=(--no-fail-fast)
  local p; for p in "$@"; do args=(-p "$p" "${args[@]}"); done
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
      -u HOMELAB_TOKEN -u HOMELAB_HOST -u HOMELAB_CONFIG -u HOMELAB_LISTEN \
      cargo test "${args[@]}" 2>&1 | tee "$log"
  return "${PIPESTATUS[0]}"
}

rust_run_exact() {  # rust_run_exact LOG PKG NAME...
  local log="$1" pkg="$2"; shift 2
  [ $# -eq 0 ] && { : > "$log"; return 0; }
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
      -u HOMELAB_TOKEN -u HOMELAB_HOST -u HOMELAB_CONFIG -u HOMELAB_LISTEN \
      cargo test -p "$pkg" --no-fail-fast -- --exact "$@" 2>&1 | tee "$log"
  return "${PIPESTATUS[0]}"
}

cmd_rust() {
  local state="$gitdir/gate-carry/rust"
  mkdir -p "$state"
  local cur_tc; cur_tc="$(rustc -V 2>/dev/null || echo 'rustc none')"
  local workdir; workdir=$(mktemp -d)
  trap 'rm -rf "$workdir"' RETURN

  local base reason
  if [ "$force_full" = full ]; then
    reason="GATE_CARRY_MODE=full"
  else
    base=$(carry_base rust "$cur_tc" 2>"$workdir/reason") || reason=$(cat "$workdir/reason")
  fi

  if [ -z "${reason:-}" ]; then
    local changed; changed=$(git diff --name-only "$base" -- . 2>/dev/null || true)
    local glob_reason
    glob_reason=$(gate_global_trigger "$changed") && reason="$glob_reason"
  fi

  if [ -z "${reason:-}" ]; then
    # Which workspace crates changed, by directory prefix.
    python3 "$meta" dir-map > "$workdir/dir-map.tsv"
    local changed_dirs changed_pkgs=()
    changed_dirs=$(awk -F'\t' '{print $1}' "$workdir/dir-map.tsv" | sort -u)
    while IFS= read -r d; do
      [ -z "$d" ] && continue
      if grep -qE "^${d//\//\\/}/" <<<"$changed" || grep -qxF "$d" <<<"$changed"; then
        local pkg; pkg=$(awk -F'\t' -v d="$d" '$1==d{print $2}' "$workdir/dir-map.tsv")
        [ -n "$pkg" ] && changed_pkgs+=("$pkg")
      fi
    done <<<"$changed_dirs"

    if [ "${#changed_pkgs[@]}" -gt 0 ]; then
      python3 "$meta" foundational "${changed_pkgs[@]}" > "$workdir/foundational.tsv" 2>/dev/null || : > "$workdir/foundational.tsv"
      if [ -s "$workdir/foundational.tsv" ]; then
        reason="$(awk -F'\t' '{printf "%s is depended on by %s of %s other workspace crates — ", $1, $2, $3}' "$workdir/foundational.tsv")changed"
      fi
    fi
  fi

  if [ -n "${reason:-}" ]; then
    echo "gate-carry: full Rust run — $reason"
    local log="$workdir/full.log"
    local rc=0; rust_run_full "$log" || rc=$?
    python3 "$meta" stem-map > "$workdir/stem-map.tsv" 2>/dev/null || : > "$workdir/stem-map.tsv"
    rust_parse_failures "$log" "$workdir/stem-map.tsv" > "$workdir/failing.tsv"
    record_state rust "$(current_tree)" "$cur_tc" "$workdir/failing.tsv" full
    {
      echo "full run, $(date -Iseconds)"
      echo "reason: $reason"
    } > "$state/last-run.txt"
    return "$rc"
  fi

  # Carry: previously-failing tests always rerun; changed, non-foundational
  # crates are rerun in full; everything else is left as the last full run
  # found it, because nothing that could affect it has moved since.
  local old_fail="$state/failing.tsv"
  [ -s "$old_fail" ] || : > "$old_fail"
  python3 "$meta" stem-map > "$workdir/stem-map.tsv" 2>/dev/null || : > "$workdir/stem-map.tsv"

  declare -A covered=()  # package -> 1 if a full -p rerun already covers it
  local full_pkgs=()
  for p in "${changed_pkgs[@]:-}"; do
    [ -z "$p" ] && continue
    full_pkgs+=("$p"); covered["$p"]=1
  done

  declare -A exact_by_pkg=()
  if [ -s "$old_fail" ]; then
    while IFS=$'\t' read -r pkg name; do
      [ -z "$pkg" ] && continue
      [ -n "${covered[$pkg]:-}" ] && continue  # already fully rerun above
      exact_by_pkg["$pkg"]="${exact_by_pkg[$pkg]:-}"$'\n'"$name"
    done < "$old_fail"
  fi

  if [ "${#full_pkgs[@]}" -eq 0 ] && [ "${#exact_by_pkg[@]}" -eq 0 ]; then
    echo "gate-carry: carried Rust run — nothing changed and nothing was failing, no tests to rerun (base $(git rev-parse --short "$base"))"
    record_state rust "$(current_tree)" "$cur_tc" /dev/null "carry:$(git rev-parse --short "$base")"
    { echo "carried run, $(date -Iseconds)"; echo "base: $base"; echo "reran: nothing (no changes, no prior failures)"; } > "$state/last-run.txt"
    return 0
  fi

  local exact_pkg_list="none"
  [ "${#exact_by_pkg[@]}" -gt 0 ] && exact_pkg_list="${!exact_by_pkg[*]}"
  echo "gate-carry: carried Rust run from $(git rev-parse --short "$base") — full: ${full_pkgs[*]:-none}; previously-failing rerun: $exact_pkg_list"
  local rc=0 combined="$workdir/combined.log"
  : > "$combined"
  if [ "${#full_pkgs[@]}" -gt 0 ]; then
    local l="$workdir/full-pkgs.log"
    rust_run_pkgs_full "$l" "${full_pkgs[@]}" || rc=$?
    cat "$l" >> "$combined"
  fi
  if [ "${#exact_by_pkg[@]}" -gt 0 ]; then
    for pkg in "${!exact_by_pkg[@]}"; do
      local names=() ; while IFS= read -r n; do [ -n "$n" ] && names+=("$n"); done <<<"${exact_by_pkg[$pkg]}"
      [ "${#names[@]}" -eq 0 ] && continue
      local l="$workdir/exact-$pkg.log"
      rust_run_exact "$l" "$pkg" "${names[@]}" || rc=$?
      cat "$l" >> "$combined"
    done
  fi

  rust_parse_failures "$combined" "$workdir/stem-map.tsv" > "$workdir/failing.tsv"
  record_state rust "$(current_tree)" "$cur_tc" "$workdir/failing.tsv" "carry:$(git rev-parse --short "$base")"
  {
    echo "carried run, $(date -Iseconds)"
    echo "base: $base"
    echo "reran in full: ${full_pkgs[*]:-none}"
    echo "reran previously-failing: ${exact_pkg_list}"
  } > "$state/last-run.txt"
  return "$rc"
}

# ── Node (admin/web) family ─────────────────────────────────────────────

node_test_cmd() {
  cd admin/web && { [ -d node_modules ] || npm ci --no-audit --no-fund; } && node --import ./test/support/kp-register.mjs --test "$@" test/
}

node_parse_failures() {
  # node --test's TAP output marks a failed case "not ok N - <name>" (possibly
  # indented under a nested describe). The full "<name>" text (parent path
  # included if node already qualifies it) becomes the rerun pattern.
  grep -E '^[[:space:]]*not ok [0-9]+ - ' "$1" | sed -E 's/^[[:space:]]*not ok [0-9]+ - //'
}

cmd_node() {
  local state="$gitdir/gate-carry/node"
  mkdir -p "$state"
  local cur_tc; cur_tc="node $(node -v 2>/dev/null || echo none)"
  local workdir; workdir=$(mktemp -d)
  trap 'rm -rf "$workdir"' RETURN

  local base reason
  if [ "$force_full" = full ]; then
    reason="GATE_CARRY_MODE=full"
  else
    base=$(carry_base node "$cur_tc" 2>"$workdir/reason") || reason=$(cat "$workdir/reason")
  fi

  local changed=""
  if [ -z "${reason:-}" ]; then
    changed=$(git diff --name-only "$base" -- admin/web 2>/dev/null || true)
  fi

  local old_fail="$state/failing.tsv"
  [ -s "$old_fail" ] || : > "$old_fail"

  if [ -z "${reason:-}" ] && [ -z "$changed" ] && [ ! -s "$old_fail" ]; then
    echo "gate-carry: node tests skipped — admin/web unchanged since $(git rev-parse --short "$base") and nothing was failing"
    record_state node "$(current_tree)" "$cur_tc" /dev/null "carry:$(git rev-parse --short "$base")"
    { echo "carried run, $(date -Iseconds)"; echo "base: $base"; echo "reran: nothing (admin/web unchanged, no prior failures)"; } > "$state/last-run.txt"
    return 0
  fi

  if [ -n "${reason:-}" ] || [ -n "$changed" ]; then
    local why="${reason:-admin/web changed since $(git rev-parse --short "$base" 2>/dev/null || echo "the last run")}"
    echo "gate-carry: full node-test run — $why"
    local log="$workdir/full.log"
    local rc=0
    node_test_cmd 2>&1 | tee "$log"; rc="${PIPESTATUS[0]}"
    node_parse_failures "$log" > "$workdir/failing.tsv"
    record_state node "$(current_tree)" "$cur_tc" "$workdir/failing.tsv" full
    { echo "full run, $(date -Iseconds)"; echo "reason: $why"; } > "$state/last-run.txt"
    return "$rc"
  fi

  # Nothing changed, but something was still marked failing: rerun exactly
  # those by name pattern (node's --test-name-pattern is a regex; each
  # recorded name is matched literally).
  local patterns=()
  while IFS= read -r n; do [ -n "$n" ] && patterns+=(--test-name-pattern "$(printf '%s' "$n" | sed -E 's/[.^$*+?()\[\]{}|\\]/\\&/g')"); done < "$old_fail"
  echo "gate-carry: carried node-test run from $(git rev-parse --short "$base") — rerunning ${#patterns[@]} previously-failing test(s)"
  local log="$workdir/carry.log" rc=0
  node_test_cmd "${patterns[@]}" 2>&1 | tee "$log"; rc="${PIPESTATUS[0]}"
  node_parse_failures "$log" > "$workdir/failing.tsv"
  record_state node "$(current_tree)" "$cur_tc" "$workdir/failing.tsv" "carry:$(git rev-parse --short "$base")"
  { echo "carried run, $(date -Iseconds)"; echo "base: $base"; echo "reran previously-failing: ${#patterns[@]}"; } > "$state/last-run.txt"
  return "$rc"
}

# ── Invariants family (docs/INVARIANTS.md) ──────────────────────────────
#
# The Playwright smoke in admin/web/test-e2e/invariants.e2e.js, against the
# admin dashboard's demo-host build (scripts/invariants-run.sh builds and
# serves it, drives the suite, tears it down). It depends on both admin/web
# (the pages it drives) and admin/src (the demo host and driver it drives
# against), so it tracks both — unlike the node family above, which only
# cares about admin/web. No sub-test carry (it is Kenny's "one short smoke
# per milestone", not a suite worth rerunning piecemeal): full run or
# skipped, same shape as rust/node's own full path, just without the
# failing-test bookkeeping.
cmd_invariants() {
  local state="$gitdir/gate-carry/invariants"
  mkdir -p "$state"
  local cur_tc; cur_tc="node $(node -v 2>/dev/null || echo none)"
  local workdir; workdir=$(mktemp -d)
  trap 'rm -rf "$workdir"' RETURN

  local base reason
  if [ "$force_full" = full ]; then
    reason="GATE_CARRY_MODE=full"
  else
    base=$(carry_base invariants "$cur_tc" 2>"$workdir/reason") || reason=$(cat "$workdir/reason")
  fi

  local changed=""
  if [ -z "${reason:-}" ]; then
    changed=$(git diff --name-only "$base" -- admin/web admin/src 2>/dev/null || true)
  fi

  if [ -z "${reason:-}" ] && [ -z "$changed" ]; then
    echo "gate-carry: invariants smoke skipped — admin/web and admin/src unchanged since $(git rev-parse --short "$base")"
    record_state invariants "$(current_tree)" "$cur_tc" /dev/null "carry:$(git rev-parse --short "$base")"
    { echo "carried run, $(date -Iseconds)"; echo "base: $base"; echo "reran: nothing (admin/web and admin/src unchanged)"; } > "$state/last-run.txt"
    return 0
  fi

  local why="${reason:-admin/web or admin/src changed since $(git rev-parse --short "$base" 2>/dev/null || echo "the last run")}"
  echo "gate-carry: invariants smoke run — $why"
  local rc=0
  "$root"/scripts/invariants-run.sh || rc=$?
  record_state invariants "$(current_tree)" "$cur_tc" /dev/null full
  { echo "full run, $(date -Iseconds)"; echo "reason: $why"; } > "$state/last-run.txt"
  return "$rc"
}

# Sourced (by the test suite, to exercise the parsing/decision functions
# directly against fixtures) vs executed: only dispatch when run as a
# command, so `source gate-carry.sh` loads the functions and nothing else.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  # Rule 5 (Kenny, 2026-10-02): every test run reports how long it took,
  # measured, per run (the line above it already says full, carried or
  # skipped).
  timed() {  # timed LABEL CMD...
    local start=$SECONDS rc=0
    "${@:2}" || rc=$?
    local d=$((SECONDS - start))
    if [ "$d" -ge 60 ]; then
      echo "gate-carry: $1 took $((d / 60)) min $((d % 60)) s (exit $rc)"
    else
      echo "gate-carry: $1 took $d s (exit $rc)"
    fi
    return "$rc"
  }
  case "${1:-}" in
    rust) timed "rust tests" cmd_rust ;;
    node) timed "node tests" cmd_node ;;
    invariants) timed "invariants smoke" cmd_invariants ;;
    full) GATE_CARRY_MODE=full; force_full=full; timed "rust tests" cmd_rust; rc1=$?; timed "node tests" cmd_node; rc2=$?; timed "invariants smoke" cmd_invariants; rc3=$?; [ "$rc1" = 0 ] && [ "$rc2" = 0 ] && [ "$rc3" = 0 ] ;;
    *) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
fi
