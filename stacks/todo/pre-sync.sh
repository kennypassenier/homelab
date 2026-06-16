#!/usr/bin/env bash
set -euo pipefail

# Ensure bind-mount targets exist and are writable by app containers running as uid=1000.
VIKUNJA_CONFIG_DIR="/appdata/todo/vikunja/config"
VIKUNJA_FILES_DIR="/appdata/todo/vikunja/files"
PROMTAIL_CONFIG_DIR="/appdata/todo/promtail-config"
PROMTAIL_CONFIG_SRC="/opt/gitops/stacks/todo/promtail-config/config.yml"
PROMTAIL_CONFIG_DST="${PROMTAIL_CONFIG_DIR}/config.yml"

mkdir -p "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"
chown -R 1000:1000 "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"
chmod 0755 "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"

mkdir -p "$PROMTAIL_CONFIG_DIR"
if [[ -f "$PROMTAIL_CONFIG_SRC" ]]; then
	install -m 0644 "$PROMTAIL_CONFIG_SRC" "$PROMTAIL_CONFIG_DST"
fi
