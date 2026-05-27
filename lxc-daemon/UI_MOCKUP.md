# LXC TUI — UI Mockup

> Binary: `apps/LXC` — runs inside each LXC container, accessed via SSH.
> Stack: Rust + ratatui, same cyberpunk palette as client-app.

---

## Dashboard tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    GitOps    Containers    Secrets    Logs                                   │
╰──────────────────────[ LXC_DAEMON :: lxc-media ]───────────────────────────────────────╯

╭──────────────────────────────[ STACK_STATUS :: media ]─────────────────────────────────╮
│  >> LXC_CORE <<   Stack: media  ·  IP: 192.168.1.104  ·  Uptime: 3d 18h               │
│  GitOps: ✓ SYNCED  ·  Last sync: 2m ago  ·  Containers: 7/7 UP  ·  Secrets: LOADED    │
╰────────────────────────────────────────────────────────────────────────────────────────╯
╔══════════════[ CONTAINERS :: 7 RUNNING ]═══════════════╗╭──[ GITOPS_ENGINE ]──────────────╮
║ STATUS   NAME              IMAGE          UPTIME  CPU  ║│ Branch:  main                  │
║────────────────────────────────────────────────────── ║│ Commit:  a3f91b2  [CLEAN]      │
║ ● UP     sonarr             lscr.io/lsio  3d 18h   2%  ║│ Sparse:  media/*               │
║ ● UP     radarr             lscr.io/lsio  3d 18h   3%  ║│ Last:    2026-05-26 14:02:11   │
║ ● UP     prowlarr           lscr.io/lsio  3d 18h   1%  ║│ Status:  ✓ UP TO DATE          │
║ ● UP     bazarr             lscr.io/lsio  3d 18h   1%  ║│                                │
║ ● UP     jellyfin           lscr.io/lsio  3d 18h  18%  ║│ API:     ● LISTENING :8080     │
║ ● UP     seerr              sctx/overseerr 3d 18h  2%  ║│ Cron:    every 30 min          │
║ ● UP     watchtower         containrrr    3d 18h   0%  ║│                                │
╚═══════════════════════════════════════════════════════╝│  [s] trigger sync now          │
                                                          │  [f] force pull + redeploy     │
  ↑/↓ select   [r] restart   [l] logs   [e] exec shell   ╰────────────────────────────────╯
  ● 0xFF4A2F :: lxc-media [ONLINE] :: 7/7 UP :: GITOPS SYNCED :: SECRETS LOADED :: ▒
```

---

## GitOps tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    [ GitOps ]    Containers    Secrets    Logs                               │
╰──────────────────────[ LXC_DAEMON :: lxc-media ]───────────────────────────────────────╯

╔═══════════════[ GITOPS_ENGINE :: media ]══════════════╗╭──[ SYNC_LOG (last 5) ]──────────────╮
║ Repo:     git@github.com:user/homelab.git             ║│ 14:02  ✓ sonarr       pulled+up    │
║ Branch:   main  ·  Commit: a3f91b2                    ║│ 14:02  ✓ radarr       up-to-date   │
║ Sparse:   media/*                                     ║│ 14:02  ✓ jellyfin     pulled+up    │
║ Lock:     /tmp/gitops.lock  ●  FREE                   ║│ 13:32  ✓ all apps     up-to-date   │
║                                                       ║│ 13:02  ✓ all apps     up-to-date   │
║ Last sync:   2026-05-26 14:02:11  (2m ago)            ║╰──────────────────────────────────╯
║ Next cron:   2026-05-26 14:32:11  (in 28m)            ║
║ Status:      ✓ SYNCED [CLEAN]                         ║╭──[ ROLLBACK_GUARD ]─────────────────╮
║                                                       ║│ Last known-good image IDs:         │
║ HTTP Push:   ● LISTENING  0.0.0.0:8080                ║│ sonarr:    sha256:4a2f...          │
║              /api/sync    /api/backup/pause           ║│ radarr:    sha256:9c1e...          │
║              /api/backup/resume                       ║│ jellyfin:  sha256:7b3a...          │
╚═══════════════════════════════════════════════════════╝│                                   │
                                                          │ Auto-rollback: ● ARMED (10s)      │
  [s] sync now   [f] force redeploy   [g] GC orphans      ╰───────────────────────────────────╯
```

---

## Containers tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    GitOps    [ Containers ]    Secrets    Logs                               │
╰──────────────────────[ LXC_DAEMON :: lxc-media ]───────────────────────────────────────╯

╔══════════════════════════════[ CONTAINER_MESH :: media :: 7/7 UP ]═══════════════════════╗
║ STATUS   NAME              IMAGE                  PORTS            CPU    RAM    UPTIME  ║
║────────────────────────────────────────────────────────────────────────────────────────  ║
║ ● UP     sonarr             lscr.io/linuxserver    :8989           2%    210 MB  3d 18h  ║
║ ● UP     radarr             lscr.io/linuxserver    :7878           3%    195 MB  3d 18h  ║
║ ● UP     prowlarr           lscr.io/linuxserver    :9696           1%    140 MB  3d 18h  ║
║ ● UP     bazarr             lscr.io/linuxserver    :6767           1%    120 MB  3d 18h  ║
║►● UP     jellyfin           lscr.io/linuxserver    :8096           18%   820 MB  3d 18h  ║
║ ● UP     seerr              sctx/overseerr         :5055           2%    310 MB  3d 18h  ║
║ ● UP     watchtower         containrrr/watchtower  (internal)      0%     48 MB  3d 18h  ║
╚══════════════════════════════════════════════════════════════════════════════════════════╝

  ↑/↓ select   [r] restart   [x] stop   [l] view logs   [e] exec shell
```

---

## Secrets tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    GitOps    Containers    [ Secrets ]    Logs                               │
╰──────────────────────[ LXC_DAEMON :: lxc-media ]───────────────────────────────────────╯

╔══════════════════[ SECRETS_ENGINE :: Ephemeral Container ]══════════════════╗
║ Method:    Ephemeral Docker container (Fail-Closed)                        ║
║ Target:    /opt/appdata/media/.env                                         ║
║ Status:    ✓ SECRETS LOADED  ·  Loaded: 3d 18h ago at boot                ║
║                                                                            ║
║  Last run:                                                                 ║
║  [14:02:05]  ● Spinning up secrets container...                            ║
║  [14:02:07]  ● Pulling secrets from vault...                               ║
║  [14:02:09]  ✓ .env written (23 keys)                                      ║
║  [14:02:09]  ✓ Ephemeral container exited cleanly                          ║
║  [14:02:09]  ✓ Mount validation passed (st_dev mismatch confirmed)         ║
║                                                                            ║
║  Mount check:  /docker   st_dev=0x0801  ✓  MOUNTED                        ║
║                /config   st_dev=0x0802  ✓  MOUNTED                        ║
╚════════════════════════════════════════════════════════════════════════════╝

  [r] reload secrets   [v] view .env keys (redacted)
```

---

## Color Reference

| Element                        | Color              |
|--------------------------------|--------------------|
| `>> LXC_CORE <<` banner        | Cyan + BOLD        |
| `╔══[...` double border        | Cyan (active)      |
| `╭──[...` rounded border       | Magenta (secondary)|
| `● UP` / `✓`                   | Green              |
| `○ DOWN` / `✗`                 | DarkGray / Red     |
| Selected row                   | Pulsing cyan bg    |
| Warnings                       | Yellow             |
| Errors / rollback active       | Red                |
| Ticker bar                     | Dark green-gray    |
