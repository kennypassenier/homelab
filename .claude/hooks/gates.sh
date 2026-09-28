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
# gap-36 (2026-09-28): the same for the daemon's own variables. The host
# reads HOMELAB_TOKEN before its config file, and a short one ends the
# process with exit 1 — so a commit made from a shell that had exported
# HOMELAB_TOKEN=offline (to regenerate the test plan) killed the host's
# unit-test binary mid-suite, with no failing test named. Six blocked
# commits in one night; reproduced with `HOMELAB_TOKEN=offline cargo test
# -p homelab-host --bin homelab-host`: "FATAL: token must be set".
# The suite also validates inputs that are not Rust: every stack file
# (core/tests/stack_files_tests.rs), the committed DR runbook and test plan
# (client/tests/tui_snapshot_tests.rs), config/client.toml
# (repo_config_tests.rs), templates and presets. Keyed on Rust alone, a
# broken stack file was committed and deployed from the working tree before
# any test saw it (expert panel 2026-09-27, local-gate-skips-stack-tests).
# The generated policy table in UPDATE_POLICY.md joined them on 2026-09-27
# (fix-144, update-policy-doc-drift): a hand edit there is caught too.
# Rule 7 as amended 2026-09-28: the commit runs the subset in
# .githooks/test-subset.sh (every security suite included); `make gate` and
# `make release` run the whole suite.
gate_glob suite '*.rs' 'Cargo.toml' 'Cargo.lock' '*/Cargo.toml' \
  'stacks/*' 'templates/*' 'presets/*' 'config/*' 'proto/*' 'core/assets/*' \
  'docs/DR_RUNBOOK.md' 'docs/deployment/TEST_PLAN.md' 'docs/deployment/UPDATE_POLICY.md' -- \
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
      -u HOMELAB_TOKEN -u HOMELAB_HOST -u HOMELAB_CONFIG -u HOMELAB_LISTEN \
      .githooks/test-subset.sh

# tech-js-checks (homelab-admin, 2026-09-28): the dashboard's browser code
# is plain ES modules; tsc checks its JSDoc types (checkJs, strict, no
# emit), prettier its layout, node --test its pure view models.
gate_glob admin-web 'admin/web/*' -- \
  sh -c 'cd admin/web && { [ -d node_modules ] || npm ci --no-audit --no-fund; } && npm run --silent check'

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
