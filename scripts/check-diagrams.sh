#!/usr/bin/env bash
# Render every ```mermaid block in the documentation and fail on the first
# one that does not parse. GitHub shows a broken diagram as an error box, and
# nothing else in the gates reads Mermaid, so this is the only check there is.
#
# Needs node (npx) and a headless Chrome for mermaid-cli:
#   npx -y puppeteer browsers install chrome-headless-shell
# Run from the repository root: scripts/check-diagrams.sh [files...]
set -euo pipefail

files=("$@")
if [ ${#files[@]} -eq 0 ]; then
  mapfile -t files < <(git ls-files 'README.md' 'docs/*.md' 'docs/deployment/*.md')
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
count=0
fail=0
for f in "${files[@]}"; do
  # Split the blocks out, one .mmd per block, named after file and line.
  awk -v out="$work" -v src="$f" '
    /^```mermaid[[:space:]]*$/ { inb=1; n++; start=NR; name=out "/" gensub(/[^A-Za-z0-9]/, "_", "g", src) "_" start ".mmd"; next }
    inb && /^```[[:space:]]*$/ { inb=0; close(name); next }
    inb { print > name }
  ' "$f"
done
for m in "$work"/*.mmd; do
  [ -e "$m" ] || continue
  count=$((count + 1))
  if ! npx -y @mermaid-js/mermaid-cli@11 -q -i "$m" -o "${m%.mmd}.svg" >"${m%.mmd}.log" 2>&1; then
    echo "BROKEN: $(basename "$m" .mmd)" >&2
    grep -m3 -iE 'error|parse' "${m%.mmd}.log" >&2 || true
    fail=$((fail + 1))
  fi
done
echo "diagrams: $count rendered, $fail broken"
[ "$fail" -eq 0 ]
