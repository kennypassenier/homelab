#!/usr/bin/env bash
# The commit-time test subset (dev-procedure rule 7 as amended 2026-09-28,
# Kenny: "een deelset bij commit … zo goed als alle tests moeten op deze
# machine gedraaid worden, niet via github actions").
#
# Every test target runs except the ones listed in SLOW below. `make gate`
# (and so `make release`) still runs the whole suite.
#
# Rule 7i, measured 2026-09-28 at 12:58 on WSL: 80 test binaries, 966 tests,
# about 49 s of run time. The three skipped binaries hold 82 tests (71 + 7 + 4)
# and about 45 s of that: they render and drive the TUI and none of them
# checks a secret, a token, TLS, the no-touch list or a stack file. Every
# security suite (secrets, latch_secrets, argv_secret, secret_gate, tls_pin,
# mock_executor) stays in.
set -euo pipefail
SLOW="tui_snapshot_tests remote_backend_tests trace_line_tests"
args=(--workspace --lib --bins)
for f in */tests/*.rs; do
  name=$(basename "$f" .rs)
  case " $SLOW " in *" $name "*) continue ;; esac
  args+=(--test "$name")
done
exec cargo test "${args[@]}" "$@"
