#!/usr/bin/env bash
set -euo pipefail

# Ensure bind-mount targets exist and are writable by app containers running as uid=1000.
VIKUNJA_CONFIG_DIR="/appdata/todo/vikunja/config"
VIKUNJA_FILES_DIR="/appdata/todo/vikunja/files"

mkdir -p "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"
chown -R 1000:1000 "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"
chmod 0755 "$VIKUNJA_CONFIG_DIR" "$VIKUNJA_FILES_DIR"
