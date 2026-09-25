#!/usr/bin/env bash
# Project quality gates (standing rule 7) — called by check-commit.sh
# before every git commit; non-zero exit blocks the commit.
set -euo pipefail

# ── Standing rule 7: a gate that does not predict the build is not a gate ──
# The checks below rewrite files. cargo updates Cargo.lock, formatters
# rewrite sources — and anything rewritten AFTER `git add` is green here
# and absent from the commit. kyu's 1.0.0 commit carried a lock file
# still naming version 0.0.0; the container build refused it one step
# before a release tag, and nothing local had objected. So: fingerprint
# the tree now, compare once the checks are done, and refuse rather than
# report a green run over a tree that moved underneath it.
gate_tree_fingerprint() {
  { git status --porcelain; git diff; } | sha256sum | cut -d' ' -f1
}
gate_tree_before=$(gate_tree_fingerprint)
# Kenny, 2026-09-16, standing rule 49 (commit-floor and rust-suite):
# format and lint always run, and the suite is skipped when no Rust
# source moved. Measured across sixteen projects: 41% of commits touch
# only documentation or configuration and paid for the suite anyway. Per
# crate was measured and rejected — `cargo test -p <crate>` is not faster
# than the whole workspace, because cargo runs every test binary either
# way.
. "$(git rev-parse --show-toplevel)/.githooks/gate-cache.sh"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
# `git commit` exports GIT_DIR (and friends) into its hooks. Tests that
# spawn a real git in a temp repo of their own inherit it and end up
# operating on THIS repo instead — core/tests/real_deps_tests.rs does
# exactly that. In a plain clone GIT_DIR is the relative ".git", which
# happens to resolve to the test's own repo and hides the problem; from a
# worktree it is absolute, and the gate then goes red on a tree that is
# fine. A gate that fails on something that is not broken teaches people
# to bypass it, so hand the suite the clean environment it assumes.
# The env stripping goes INSIDE the gate, not around it: `env` starts a
# program and `gate_glob` is a shell function, so the wrapper that way
# round printed `env: 'gate_glob': No such file or directory` and the
# suite never ran (2026-09-16).
gate_glob suite '*.rs' 'Cargo.toml' 'Cargo.lock' '*/Cargo.toml' -- \
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
      cargo test --workspace

gate_cache_done

# Standing rule 7, second clause: see gate_tree_fingerprint above.
if [ "$(gate_tree_fingerprint)" != "$gate_tree_before" ]; then
  {
    echo "gates: the checks rewrote the working tree while they ran."
    echo "A file changed after it was staged, so what this commit carries is"
    echo "NOT what was just tested. Most often this is cargo refreshing"
    echo "Cargo.lock; the changed paths are listed below."
    echo
    git status --porcelain
    echo
    echo "What now: run 'git add -A' and commit again."
  } >&2
  exit 1
fi
