#!/bin/bash
set -e
# Only seed config if it doesn't exist
CONF="/appdata/downloader/qbittorrent/config/qBittorrent/qBittorrent.conf"
TEMPLATE="/appdata/downloader/qBittorrent.conf.template"
if [ ! -f "$CONF" ] && [ -f "$TEMPLATE" ]; then
  cp "$TEMPLATE" "$CONF"
fi
