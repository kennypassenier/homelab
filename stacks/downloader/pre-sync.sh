#!/usr/bin/env bash
# Script Name: pre-sync.sh
# Description: Pre-sync hook for the downloader stack. Automatically executed by node-sync.sh
#              before every docker compose deploy.
#              Keeps VueTorrent (alternative qBittorrent Web UI) up-to-date by cloning on
#              first run and pulling on every subsequent run. Because node-sync.sh runs every
#              5 minutes, this effectively gives us automatic VueTorrent updates for free.
#
#              Two directories are used:
#              - vuetorrent-git: the actual git clone (contains .git — never mounted)
#              - vuetorrent:     clean web files only, rsync'd from vuetorrent-git
#                                (no .git dir, no non-regular files — safe for qBittorrent)

set -euo pipefail

VUETORRENT_GIT="/appdata/downloader/vuetorrent-git"
VUETORRENT_WEB="/appdata/downloader/vuetorrent"
VUETORRENT_REPO="https://github.com/VueTorrent/VueTorrent.git"

if [[ ! -d "${VUETORRENT_GIT}/.git" ]]; then
    echo "[pre-sync] VueTorrent not found. Cloning latest-release branch..."
    git clone --single-branch --branch latest-release "${VUETORRENT_REPO}" "${VUETORRENT_GIT}"
    echo "[pre-sync] VueTorrent cloned successfully."
    VUETORRENT_UPDATED=true
else
    echo "[pre-sync] VueTorrent found. Pulling latest updates..."
    pull_output=$(git -C "${VUETORRENT_GIT}" pull --ff-only)
    echo "[pre-sync] ${pull_output}"
    if echo "${pull_output}" | grep -q "Already up to date."; then
        VUETORRENT_UPDATED=false
    else
        VUETORRENT_UPDATED=true
    fi
fi

# Only rsync when there are actual changes — avoids unnecessary I/O on every 5-minute sync.
# Running rsync while qBittorrent is live is safe: these are static files read on-request.
if [[ "${VUETORRENT_UPDATED}" == true ]]; then
    echo "[pre-sync] Syncing updated web files to ${VUETORRENT_WEB}..."
    rsync -a --delete --exclude='.git' "${VUETORRENT_GIT}/" "${VUETORRENT_WEB}/"
    echo "[pre-sync] VueTorrent web files ready."
else
    echo "[pre-sync] VueTorrent unchanged. Skipping rsync."
fi
