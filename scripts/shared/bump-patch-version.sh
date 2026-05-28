#!/usr/bin/env bash
# =============================================================================
# bump-patch-version.sh
# Auto-increment patch version in Cargo.toml if major/minor haven't changed
# since the last git tag.
#
# Usage: ./bump-patch-version.sh <path-to-Cargo.toml> <component-name>
#   e.g., ./bump-patch-version.sh host-daemon/Cargo.toml HOST
#
# Exit codes:
#   0 = version bumped successfully
#   1 = no prior tag found; version not incremented
#   2 = major/minor version changed; manual version management required
# =============================================================================

set -e

CARGO_TOML="$1"
COMPONENT_NAME="${2:-Component}"
COMPONENT_TAG_PREFIX="${COMPONENT_NAME,,}-daemon"  # e.g., "host-daemon" or "lxc-daemon"

if [[ ! -f "$CARGO_TOML" ]]; then
    echo "✗ Cargo.toml not found at: $CARGO_TOML" >&2
    exit 1
fi

# Extract current version from Cargo.toml
CURRENT_VERSION=$(grep '^version' "$CARGO_TOML" | head -1 | cut -d'"' -f2)
if [[ -z "$CURRENT_VERSION" ]]; then
    echo "✗ Could not extract version from $CARGO_TOML" >&2
    exit 1
fi

# Parse major.minor.patch
IFS='.' read -r CURRENT_MAJOR CURRENT_MINOR CURRENT_PATCH <<< "$CURRENT_VERSION"

# Find the latest git tag for this component
LATEST_TAG=$(git tag -l "${COMPONENT_TAG_PREFIX}-v*" --sort=-version:refname | head -1)

if [[ -z "$LATEST_TAG" ]]; then
    echo "⚠ No prior tag found for $COMPONENT_NAME; skipping version bump"
    exit 1
fi

# Extract version from tag (e.g., "host-daemon-v1.2.3" → "1.2.3")
TAGGED_VERSION="${LATEST_TAG##*-v}"
IFS='.' read -r TAGGED_MAJOR TAGGED_MINOR TAGGED_PATCH <<< "$TAGGED_VERSION"

# Check if major or minor changed
if [[ "$CURRENT_MAJOR" != "$TAGGED_MAJOR" ]] || [[ "$CURRENT_MINOR" != "$TAGGED_MINOR" ]]; then
    echo "⚠ Major or minor version changed (was $TAGGED_VERSION, now $CURRENT_VERSION);"
    echo "   Manual version management detected; not auto-incrementing patch"
    exit 2
fi

# Auto-increment patch version
NEW_PATCH=$((TAGGED_PATCH + 1))
NEW_VERSION="${CURRENT_MAJOR}.${CURRENT_MINOR}.${NEW_PATCH}"

# Update Cargo.toml in-place
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
rm -f "$CARGO_TOML.bak"

echo "✓ $COMPONENT_NAME version bumped: $CURRENT_VERSION → $NEW_VERSION"
exit 0
