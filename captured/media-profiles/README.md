# Quality profiles as they were before Recyclarr

Exported 2026-09-04 from the live Sonarr and Radarr on CT 106, straight from
their own APIs, before anything on this machine had been changed by
Recyclarr. Both applications had **zero custom formats** at that moment.

    radarr  id=4 "1080p"     ← 961 films point at this
            id=5 "Ultra-HD"  ← 0 films
    sonarr  id=4 "1080p"     ← 210 series point at this
            id=5 "Ultra-HD"  ← 0 series

## Why this is a copy and not a rename

Kenny asked whether the old profiles could be renamed — `1080p` becoming
`1080p-archived` — so there is something to fall back on. Measured first, and
a rename is the wrong lever: films and series point at a profile by **id**,
never by name. Renaming id=4 would take all 961 films with it into the
archive, and they would then have to be moved back one by one.

A copy has none of that: the ids stay where they are, nothing moves, and a
restore is a PUT of the JSON beside this file.

## Restoring one

    curl -X PUT -H "X-Api-Key: $KEY" -H "Content-Type: application/json" \
      --data @radarr-qualityprofiles.json \
      http://10.10.10.6:7878/api/v3/qualityprofile/4

The file holds an array; restore one profile by sending that profile's own
object. Run it from CT 106 or the Proxmox host — the API keys live in each
application's `config.xml` under `/appdata/media/<app>-config/`.
