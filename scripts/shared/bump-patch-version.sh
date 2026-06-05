#!/usr/bin/env bash
# =============================================================================
# bump-patch-version.sh — idempotent patch-version incrementor
#
# Usage: ./bump-patch-version.sh <Cargo.toml> <component-name> [tag-prefix]
#   host-daemon/Cargo.toml  HOST                          → prefix: host-daemon
#   client-app/Cargo.toml   CLIENT  client                → prefix: client
#   lxc-daemon/Cargo.toml   LXC     lxc-daemon            → prefix: lxc-daemon
#
# Idempotency contract:
#   No prior tag          → use current Cargo.toml version as baseline (no bump, exit 0)
#   Cargo.toml == tag     → standard case: bump patch by 1               (exit 0)
#   Cargo.toml >  tag     → previous run was interrupted; already bumped  (no-op, exit 0)
#   Cargo.toml <  tag     → Cargo.toml is stale; advance to tag+1 patch   (exit 0)
#
# Exit codes:  0 = success (Cargo.toml is ready)   1 = fatal error
# =============================================================================
set -euo pipefail

CARGO_TOML="${1:?Usage: $0 <Cargo.toml> <name> [tag-prefix]}"
COMPONENT_NAME="${2:?Usage: $0 <Cargo.toml> <name> [tag-prefix]}"
TAG_PREFIX="${3:-${COMPONENT_NAME,,}-daemon}"

[[ -f "$CARGO_TOML" ]] || { echo "✗ $CARGO_TOML not found" >&2; exit 1; }

CURRENT=$(grep '^version' "$CARGO_TOML" | head -1 | cut -d'"' -f2)
[[ -n "$CURRENT" ]] || { echo "✗ Could not read version from $CARGO_TOML" >&2; exit 1; }
IFS='.' read -r C_MAJ C_MIN C_PAT <<< "$CURRENT"

LATEST_TAG=$(git tag -l "${TAG_PREFIX}-v*" --sort=-version:refname 2>/dev/null | head -1)

if [[ -z "$LATEST_TAG" ]]; then
    echo "✓ $COMPONENT_NAME: no prior tag — using $CURRENT as baseline"
    exit 0
fi

TAGGED="${LATEST_TAG##*-v}"
IFS='.' read -r T_MAJ T_MIN T_PAT <<< "$TAGGED"

# Numeric three-way compare: prints "gt", "eq", or "lt"
cmp3() {
    if   (( $1 > $4 )); then echo gt
    elif (( $1 < $4 )); then echo lt
    elif (( $2 > $5 )); then echo gt
    elif (( $2 < $5 )); then echo lt
    elif (( $3 > $6 )); then echo gt
    elif (( $3 < $6 )); then echo lt
    else echo eq; fi
}

case $(cmp3 "$C_MAJ" "$C_MIN" "$C_PAT" "$T_MAJ" "$T_MIN" "$T_PAT") in
    gt)
        # Cargo.toml already ahead — previous run bumped but didn't complete; no-op
        echo "✓ $COMPONENT_NAME: already at $CURRENT (ahead of tag $TAGGED) — no bump needed"
        ;;
    eq)
        # Standard path: bump patch
        NEW="${T_MAJ}.${T_MIN}.$((T_PAT + 1))"
        sed -i.bak "s/^version = \"$CURRENT\"/version = \"$NEW\"/" "$CARGO_TOML"
        rm -f "$CARGO_TOML.bak"
        echo "✓ $COMPONENT_NAME version bumped: $CURRENT → $NEW"
        ;;
    lt)
        # Cargo.toml is behind the latest tag (unusual — advance to tag+1)
        NEW="${T_MAJ}.${T_MIN}.$((T_PAT + 1))"
        sed -i.bak "s/^version = \"$CURRENT\"/version = \"$NEW\"/" "$CARGO_TOML"
        rm -f "$CARGO_TOML.bak"
        echo "✓ $COMPONENT_NAME: Cargo.toml ($CURRENT) was behind tag ($TAGGED) — advanced to $NEW"
        ;;
esac

exit 0
