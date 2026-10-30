#!/usr/bin/env bash
# A warning, never a gate (Kenny, 2026-10-30): a source file that is, or grows
# past, 1000 lines in this commit is named, so the habit of putting things in
# the right file and splitting before a file grows stays visible. The target
# is 250 lines per file and functions under about 40 lines; 1000 is the alarm.
set -uo pipefail

limit=${SIZE_WARN_LINES:-1000}
root=$(git rev-parse --show-toplevel)
cd "$root"

warned=0
while IFS= read -r file; do
  [ -f "$file" ] || continue
  lines=$(wc -l < "$file")
  if [ "$lines" -gt "$limit" ]; then
    [ "$warned" -eq 0 ] && echo "size warning (not a gate): files over $limit lines in this commit:" >&2
    echo "  $file: $lines lines" >&2
    warned=1
  fi
done < <(git diff --cached --name-only --diff-filter=AM | grep -E '\.(rs|js|css|sh|py)$' | grep -vE '(^|/)(node_modules|target)/')

if [ "$warned" -eq 1 ]; then
  echo "  Split by responsibility when you touch them; see the modular-code rule." >&2
fi
exit 0
