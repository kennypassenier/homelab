#!/usr/bin/env bash
# The internal-address gate (public-repo-attack-map, Kenny's deep-dive
# "only replace the internal addresses", 2026-10-01).
#
# This repository is public. docs/**, README.md, CLAUDE.md and
# captured/**/*.md name machines by role ("pve", "the router", "CT 109")
# instead of their real internal address, so the docs do not hand a reader
# a map of the home network. This gate refuses a commit that ADDS a
# 10.x.x.x, 192.168.x.x or 172.16-31.x.x address (bare, CIDR, or
# host:port) back into one of those files. `stacks/**`, code and tests are
# untouched by this gate — they need the real addresses.
#
# The RFC 5737 documentation ranges (192.0.2.0/24, 198.51.100.0/24,
# 203.0.113.0/24) are allowlisted: that is what docs use in their place
# when the literal shape of an address is the point of an example (see
# docs/USER_GUIDE.md).
#
# Write the machine's name instead, or — if the address really is the
# point of the example — 192.0.2.x / 198.51.100.x (RFC 5737).
# A false alarm can be committed with --no-verify, like every other gate
# here.
#
# For tests: CHECK_INTERNAL_IPS_DIFF_FILE replaces `git diff --cached`.
set -u

if [ -n "${CHECK_INTERNAL_IPS_DIFF_FILE:-}" ]; then
  diff=$(cat "$CHECK_INTERNAL_IPS_DIFF_FILE")
else
  diff=$(git diff --cached -U0 --no-color --no-ext-diff \
    -- 'docs/**/*.md' 'README.md' 'CLAUDE.md' 'captured/**/*.md')
fi

# Added lines in the scoped files only (the -- pathspec above already
# limits `git diff`; when fed a pre-built diff file for tests, every
# line in it is in scope).
added=$(printf '%s\n' "$diff" | grep -E '^\+' | grep -vE '^\+\+\+ ')

ip_re='(10(\.[0-9]{1,3}){3}|192\.168(\.[0-9]{1,3}){2}|172\.(1[6-9]|2[0-9]|3[01])(\.[0-9]{1,3}){2})(/[0-9]{1,2})?(:[0-9]{1,5})?'

hits=$(printf '%s\n' "$added" \
  | grep -nE "$ip_re" \
  | grep -vE '192\.0\.2\.[0-9]{1,3}|198\.51\.100\.[0-9]{1,3}|203\.0\.113\.[0-9]{1,3}' \
  || true)

if [ -n "$hits" ]; then
  echo "COMMIT BLOCKED — an internal network address is being added to public docs." >&2
  printf '%s\n' "$hits" | cut -c1-200 >&2
  echo "Remedy: name the machine instead (pve, the router, CT <vmid> / the" >&2
  echo "stack name, the dashboard (CT 120), the workstation, ...), or if the" >&2
  echo "literal address is the point of the example use the RFC 5737" >&2
  echo "documentation range (192.0.2.x / 198.51.100.x). This repository is" >&2
  echo "public; real addresses stay in stacks/** and code." >&2
  exit 1
fi
exit 0
