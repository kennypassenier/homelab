# Runbook: Jellyfin 10.11.x → 12.x

Researched 2026-10-02 (release notes, jellyfin.org blog, GitHub issues). The
upgrade is a database migration with no way back except a full restore, so it
is done together with Kenny, who tests the TV and phone clients afterwards.

## Decision points

- **Go straight to the newest 12.x; never boot 12.0 first.** Direct upgrades
  from 10.10.7/10.11.x are supported. 12.0 failed to start for users coming
  from 10.11.11 (FOREIGN KEY errors: jellyfin#17830, #17874, #17875, #17863);
  12.1 fixed that (#17835) and the pre-migration backup integrity (#17836).
- **Open in 12.1 on 2026-10-02:** #18098 (80% of playlists wiped from
  10.11.11), #18100 (jellyfin.db corrupted if anything opens it between the
  migrating boot and the next restart), #18210 (silent deadlock),
  #18066 (100% CPU hang). Milestone v12.2 holds fixes, no date.

## Steps

1. Read Health and the media stack's logs on 10.11.x. Save a copy of
   `config/encoding.xml` (#17861 can reset transcoding settings) and note the
   hardware-acceleration settings. Note the counts of films, episodes,
   playlists and collections, to compare in step 9.
2. In Jellyfin: **remove** every third-party plugin (disabling is not enough,
   per the Jellyfin team in #17840). Set the plugin repository to the stable
   manifest `https://repo.jellyfin.org/files/plugin/manifest.json`.
3. Stop the container. Take the backup: the media stack's own Backup action
   (restic, `jellyfin-config`, 8.8 GB), plus a storage snapshot of the
   container if the storage allows it (measure first: the containers live on
   local-lvm, not ZFS).
4. On a **copy** of jellyfin.db, never the live file:
   - `PRAGMA integrity_check;`
   - `SELECT lower(Username), count(*) FROM Users GROUP BY 1 HAVING count(*) > 1;`
     (usernames that differ only by case make the migration fail; rename first)
5. Check every media mount is present at the same path (#18099: a library was
   purged when its paths did not resolve on first boot).
6. Pin the image to the exact 12.x tag and digest in the stack file, commit,
   deploy. Follow the logs; **do not interrupt** the migration. No CPU, no disk
   I/O and no log line for a long time means a hang: restore (step 3).
7. Once the server is up, restart it once, cleanly, **before** anything
   touches jellyfin.db (#18100). Then integrity-check a copy again.
8. Restore `encoding.xml` settings if they were reset; test hardware
   transcoding.
9. Run a full library scan (required: it rebuilds alternate versions; the
   first scan is slow and some films show as newly added). Check films,
   episodes, playlists and collections against the counts noted in step 1.
10. Re-add plugins one at a time. Kenny tests the TV and phone apps.
    `/emby/*` and `/mediabrowser/*` are gone for good; legacy authorization
    can be turned back on temporarily with
    `<EnableLegacyAuthorization>true</EnableLegacyAuthorization>` in
    `system.xml` if an older client needs it.

## Rollback

Stop the container, restore `jellyfin-config` from the step-3 snapshot,
pin the previous 10.11.x image and digest back, deploy.

## Inventory before the upgrade (read from the API, 2026-10-02)

Jellyfin 10.11.11, one user (Kenny), 953 films, 214 series, 5745 episodes,
65 collections, no playlists. Every plugin was Active. Each plugin's
configuration was saved through `GET /Plugins/{id}/Configuration` to
`~/.local/share/homelab/jellyfin-12-upgrade/` on the workstation (mode 700,
outside the repository: Trakt, Open Subtitles and Webhook settings hold
credentials). The same settings also live in `/config/plugins/configurations/`
inside the backup.

Built in (cannot be removed, ship with the server): AudioDB, MusicBrainz,
OMDb, Studio Images, TMDb.

Third-party, removed before the migration and re-added one at a time:

| Plugin | 10.11 version | Repository | Build for 12.x |
|---|---|---|---|
| Fanart | 14.0.0.0 | Jellyfin Stable | 15.0.0.0 (ABI 12.0) |
| Open Subtitles | 24.0.0.0 | Jellyfin Stable | 25.0.0.0 (ABI 12.0) |
| Playback Reporting | 17.0.0.0 | Jellyfin Stable | 19.0.0.0 (ABI 12.0) |
| TheTVDB | 22.0.0.0 | Jellyfin Stable | 24.0.0.0 (ABI 12.0) |
| TMDb Box Sets | 13.0.0.0 | Jellyfin Stable | 15.0.0.0 (ABI 12.1) |
| Trakt | 30.0.0.0 | Jellyfin Stable | 33.0.0.0 (ABI 12.1) |
| Webhook | 21.0.0.0 | Jellyfin Stable | 22.0.0.0 (ABI 12.0) |
| File Transformation | 3.0.1.0 | iamparadox.dev | 3.0.1.0 (ABI 12.1) |
| Jellyfin Enhanced | 12.9.0.0 | n00bcodr (`main/manifest.json`) | 12.9.0.0 (ABI 12.0) |
| Jellyfin Tweaks | 5.0.0.0 | n00bcodr (`main/10.11/manifest.json`) | 5.0.0.0 (ABI 12.0) in `main/manifest.json` or `main/12/manifest.json`: switch the repository URL |
| Intro Skipper | 1.10.11.24 | intro-skipper.org | v12.0.4.0 in `raw.githubusercontent.com/intro-skipper/manifest/main/12.0/manifest.json`: switch the repository URL |
| Actor Plus | 1.0.0.0 | Druidblack | the manifest lists 1.0.0.1 for 12.0 under another plugin id: reinstall, then re-enter its settings |

The "danieladov" repository is configured but provides none of the
installed plugins.
