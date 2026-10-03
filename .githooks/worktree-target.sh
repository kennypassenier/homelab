#!/usr/bin/env bash
# Which cargo target directory the gates of THIS checkout build in.
#
# fix-guards-9 (coordinator, 2026-10-03): ~/.cargo/config.toml points every
# project at one shared target directory. Two linked worktrees of this
# repository committing at the same moment then built in the same
# directory, and one commit hook compiled the other tree's sources: it
# failed on a `proof` field of homelab_proto::Command that its own tree
# does not have. A gate that fails on another tree's code says nothing
# about this one.
#
# Prints the directory to use, or nothing when cargo's own choice stands:
#   * CARGO_TARGET_DIR already set   → nothing (an explicit choice wins);
#   * the main checkout              → nothing (it keeps the shared,
#                                      incremental directory it always had);
#   * a linked worktree              → $HOMELAB_TARGETS/wt-<name>-<hash>,
#                                      one per worktree path.
#
# Disk: every call first removes the directory of a worktree that no longer
# exists (each directory records its worktree in `.worktree`), then keeps
# at most HOMELAB_TARGETS_KEEP (default 4) directories, dropping the least
# recently used. One worktree's directory measured 6.1 GB after clippy over
# every target and the core, client and admin test builds (2026-10-03), so
# the cap bounds the whole at about 25 GB.
set -euo pipefail

root_dir=${HOMELAB_TARGETS:-$HOME/.cache/homelab-targets}
keep=${HOMELAB_TARGETS_KEEP:-4}

# Clean first, whatever this checkout is: a removed worktree's build output
# is never read again.
if [ -d "$root_dir" ]; then
  for d in "$root_dir"/wt-*; do
    [ -d "$d" ] || continue
    wt=$(cat "$d/.worktree" 2>/dev/null || true)
    if [ -z "$wt" ] || [ ! -e "$wt/.git" ]; then
      rm -rf -- "$d"
    fi
  done
fi

[ -n "${CARGO_TARGET_DIR:-}" ] && exit 0

top=$(git rev-parse --show-toplevel)
git_dir=$(git rev-parse --absolute-git-dir)
common=$(cd "$(git rev-parse --git-common-dir)" && pwd -P)
# The main checkout's git dir IS the common dir; a linked worktree's is
# <common>/worktrees/<name>.
[ "$(cd "$git_dir" && pwd -P)" = "$common" ] && exit 0

hash=$(printf '%s' "$top" | sha256sum | cut -c1-10)
dir="$root_dir/wt-$(basename "$top")-$hash"
mkdir -p "$dir"
printf '%s\n' "$top" >"$dir/.worktree"

# The cap: least recently used first (the marker is touched on every use).
# shellcheck disable=SC2012
ls -1t "$root_dir"/wt-*/.worktree 2>/dev/null | tail -n "+$((keep + 1))" | while read -r old; do
  old=${old%/.worktree}
  [ "$old" = "$dir" ] || rm -rf -- "$old"
done

printf '%s\n' "$dir"
