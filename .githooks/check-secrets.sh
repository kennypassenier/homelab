#!/usr/bin/env bash
# The secret gate (fix-25, Kenny 2026-09-26).
#
# This repository is public. On 2026-08-11 a Home Assistant webhook id went
# into three documents and stayed there for six weeks, while the captured
# kyu-runner config two directories away redacted its ids with the comment
# "they are the credential for Home Assistant's notification dispatcher".
# The rule was known; nothing held it up. This does, for the shape that
# leaked: `/api/webhook/<name>-<8 hex>` on an ADDED line.
#
# Write `<id>` or `REDACTED` in its place. A false alarm can be committed
# with --no-verify, like every other gate here.
#
# For tests: CHECK_SECRETS_DIFF_FILE replaces `git diff --cached`.
set -u

if [ -n "${CHECK_SECRETS_DIFF_FILE:-}" ]; then
  diff=$(cat "$CHECK_SECRETS_DIFF_FILE")
else
  diff=$(git diff --cached -U0 --no-color --no-ext-diff)
fi

hits=$(printf '%s\n' "$diff" \
  | grep -E '^\+' | grep -vE '^\+\+\+ ' \
  | grep -nE 'webhook/[a-z0-9_-]+-[0-9a-f]{8}([^0-9a-z]|$)' || true)

if [ -n "$hits" ]; then
  echo "COMMIT BLOCKED — a Home Assistant webhook id is being added (fix-25)." >&2
  printf '%s\n' "$hits" | cut -c1-160 >&2
  echo "Remedy: write <id> or REDACTED instead; this repository is public." >&2
  exit 1
fi
exit 0
