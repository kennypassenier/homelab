#!/usr/bin/env bash
# The commit-time test subset (dev-procedure rule 7 as amended 2026-09-28,
# Kenny: "een deelset bij commit … zo goed als alle tests moeten op deze
# machine gedraaid worden, niet via github actions").
#
# Every test target runs except the ones listed in SLOW below. `make gate`
# (and so `make release`) still runs the whole suite.
#
# Rule 7i, measured 2026-09-28 at 12:58 on WSL: 80 test binaries, 966 tests,
# about 49 s of run time. Skipped: tui_snapshot_tests (71 tests, about 34 s)
# and trace_line_tests (4, about 2 s), which render the TUI and read log
# lines. remote_backend_tests (7, about 10 s) was skipped at first and is back
# in: it drives the TUI's TLS handshake, and the full suite caught a rustls
# panic there that this subset had let through (commit 464ac9e). Every
# security suite (secrets, latch_secrets, argv_secret, secret_gate, tls_pin,
# mock_executor, remote_backend) runs.
set -euo pipefail
SLOW="tui_snapshot_tests trace_line_tests"
args=(--workspace --lib --bins)
for f in */tests/*.rs; do
  name=$(basename "$f" .rs)
  case " $SLOW " in *" $name "*) continue ;; esac
  args+=(--test "$name")
done
cargo test "${args[@]}" "$@"
# The skipped TUI suite also holds three cheap guards the commit must not
# lose: the committed runbook and test plan match a fresh generation, and
# every host operation is reachable from the TUI or written down as CLI-only.
# Missing them let a stale runbook through to `make release` on 2026-09-28.
exec cargo test -p homelab-client --test tui_snapshot_tests "$@" -- \
  the_committed_runbook_matches_a_fresh_generation \
  the_committed_test_plan_matches_a_fresh_generation \
  every_host_operation_is_reachable_or_deliberately_not
