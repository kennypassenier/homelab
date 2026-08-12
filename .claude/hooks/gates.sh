#!/usr/bin/env bash
# Project quality gates (standing rule 7) — called by check-commit.sh
# before every git commit; non-zero exit blocks the commit.
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
