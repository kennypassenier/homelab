#!/usr/bin/env bash
# Homelab companion (G9): publish your Rust service's Docker image to GHCR
# from your own machine, so the homelab can deploy/update it like any other
# app. Copy to scripts/publish-image.sh in your Rust repo and run it after
# tagging a release:
#
#   scripts/publish-image.sh v1.2.3
#
# It builds from the tagged tree and pushes ghcr.io/<owner>/<repo>:<version>
# and :latest, the tags release-image.yml gave (Kenny, 2026-09-29: builds run
# locally, GitHub only receives the result). Needs docker, and docker logged
# in to ghcr.io with a token that may write packages.
#
# The package is linked to the repository and takes its visibility, so a
# public repo yields a package the homelab host can pull anonymously. If a
# host ever cannot pull: repo → Packages → package settings.
set -euo pipefail
tag="${1:?usage: publish-image.sh vX.Y.Z}"
case "$tag" in v[0-9]*.[0-9]*.[0-9]*) ;; *) echo "tag must look like vX.Y.Z" >&2; exit 1 ;; esac
cd "$(git rev-parse --show-toplevel)"
[ -z "$(git status --porcelain)" ] || { echo "working tree not clean; build from the tagged tree" >&2; exit 1; }
[ "$(git rev-parse HEAD)" = "$(git rev-parse "$tag^{commit}")" ] || { echo "HEAD is not $tag; check it out first" >&2; exit 1; }
repo=$(git remote get-url origin | sed -E 's#(git@github.com:|https://github.com/)##; s#\.git$##' | tr '[:upper:]' '[:lower:]')
image="ghcr.io/$repo"
docker build -t "$image:${tag#v}" -t "$image:latest" \
  --label "org.opencontainers.image.source=https://github.com/$repo" \
  --label "org.opencontainers.image.version=${tag#v}" \
  --label "org.opencontainers.image.revision=$(git rev-parse HEAD)" .
if [ "${DRY_RUN:-0}" = 1 ]; then echo "DRY_RUN: built $image:${tag#v}, pushed nothing"; exit 0; fi
docker push "$image:${tag#v}" && docker push "$image:latest" || {
  echo "push refused: log docker in to ghcr.io with a token that has write:packages" >&2; exit 1; }
echo "published $image:${tag#v} and :latest"
