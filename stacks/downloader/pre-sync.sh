#!/usr/bin/env bash
# Script Name: pre-sync.sh
# Description: Pre-sync hook for the downloader stack. Automatically executed by node-sync.sh
#              before every docker compose deploy.
#              Keeps VueTorrent (alternative qBittorrent Web UI) up-to-date by cloning on
#              first run and pulling on every subsequent run. Because node-sync.sh runs every
#              5 minutes, this effectively gives us automatic VueTorrent updates for free.

set -euo pipefail

VUETORRENT_DIR="/appdata/downloader/vuetorrent"
VUETORRENT_REPO="https://github.com/VueTorrent/VueTorrent.git"

if [[ ! -d "${VUETORRENT_DIR}/.git" ]]; then
    echo "[pre-sync] VueTorrent not found. Cloning latest-release branch..."
    git clone --single-branch --branch latest-release "${VUETORRENT_REPO}" "${VUETORRENT_DIR}"
    echo "[pre-sync] VueTorrent cloned successfully."
else
    echo "[pre-sync] VueTorrent found. Pulling latest updates..."
    git -C "${VUETORRENT_DIR}" pull --ff-only
    echo "[pre-sync] VueTorrent up-to-date."
fi
