# HOST TUI — UI Mockup

> Binary: `apps/HOST` — runs on Proxmox bare-metal.
> Stack: Rust + ratatui, same cyberpunk palette as client-app.

---

## Dashboard tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    LXC Nodes    Backups    Storage    Hardware                                │
╰────────────────────[ HOST_DAEMON :: pve-01 ]────────────────────────────────────────────╯

╭─────────────────────────────────[ NODE_STATUS :: pve-01 ]──────────────────────────────╮
│  >> HOST_MESH <<   IP: 192.168.1.10  ·  Uptime: 47d 12h  ·  Kernel: 6.8.12-4-pve     │
│  CPU: ▃▄▅▃▄▅▄▃  14%   RAM: ▄▅▄▅▄▅▄▅  6.2/32 GB   DISK: ██████░░  214/512 GB (42%)    │
╰────────────────────────────────────────────────────────────────────────────────────────╯
╔══════════════[ LXC_MESH :: 6 NODES ]══════════╗  ╭──[ SELF_UPDATE :: CI/CD ]──────────╮
║ STATUS  ID   CONTAINER       CPU    RAM        ║  │ Current:  v1.4.2                  │
║──────────────────────────────────────────────  ║  │ Latest:   v1.5.0  ● AVAILABLE     │
║ ● RUN   101  lxc-cloudflared  3%  128/512 MB   ║  │ Channel:  github releases          │
║ ● RUN   102  lxc-downloader   8%  210/512 MB   ║  │ Status:   [IDLE]                  │
║ ● RUN   103  lxc-gateway     22%  380/1024 MB  ║  │                                   │
║ ○ STP   104  lxc-media        0%   --/1024 MB  ║  │  [u] update now                   │
║ ● RUN   105  lxc-monitoring  11%  290/512 MB   ║  ╰───────────────────────────────────╯
║ ● RUN   106  lxc-paperless   17%  640/1024 MB  ║  ╭──[ BACKUP_STATUS :: Restic ]──────╮
╚═══════════════════════════════════════════════╝  │ Last run:  2026-05-26 03:00        │
                                                    │ Duration:  4m 32s                  │
  [Enter] manage selected   [s] start   [x] stop    │ Size:      14.2 GB (dedup)         │
  [n] new LXC   [d] delete   [r] restart             │ Snapshots: 42                      │
                                                    │ Status:  ✓ OK                      │
                                                    │  [b] run backup now                │
                                                    ╰───────────────────────────────────╯
  ● 0xFF4A2F :: pve-01 [ONLINE] :: 5/6 LXC RUN :: RESTIC OK :: DISK 42% :: TEMP 51°C :: ▒
```

---

## LXC Nodes tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    [ LXC Nodes ]    Backups    Storage    Hardware                           │
╰────────────────────[ HOST_DAEMON :: pve-01 ]────────────────────────────────────────────╯

╔══════════════════════════════[ LXC_NODES :: 6 TOTAL ]══════════════════╗╭──[ DETAIL ]───────────╮
║ STATUS  ID   CONTAINER       IP               CPU    RAM      UPTIME  ║│ lxc-gateway           │
║─────────────────────────────────────────────────────────────────────  ║│──────────────────────  │
║ ● RUN   101  lxc-cloudflared  192.168.1.101    3%  128 MB   47d 12h  ║│ VMID:   103            │
║ ● RUN   102  lxc-downloader   192.168.1.102    8%  210 MB   12d  3h  ║│ Stack:  gateway        │
║►● RUN   103  lxc-gateway      192.168.1.103   22%  380 MB    3d 18h  ║│ Disk:   4.2/8 GB       │
║ ○ STP   104  lxc-media        192.168.1.104    0%    --      0d  0h  ║│ Cores:  2              │
║ ● RUN   105  lxc-monitoring   192.168.1.105   11%  290 MB   31d  0h  ║│ RAM:    1024 MB        │
║ ● RUN   106  lxc-paperless    192.168.1.106   17%  640 MB   47d 12h  ║│ GPU:    ✗              │
╚══════════════════════════════════════════════════════════════════════╝│ TUN:    ✓              │
                                                                         │ State:  ● RUNNING      │
  ↑/↓ select   [s] start   [x] stop   [r] restart   [n] provision new   │                       │
  [d] delete   [e] exec shell   [p] hardware passthrough                 │ [s] stop               │
                                                                         │ [e] exec shell         │
                                                                         │ [p] passthrough        │
                                                                         ╰───────────────────────╯
```

---

## Backups tab

```
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  Dashboard    LXC Nodes    [ Backups ]    Storage    Hardware                           │
╰────────────────────[ HOST_DAEMON :: pve-01 ]────────────────────────────────────────────╯

╔═════════════════[ BACKUP_ORCHESTRATOR :: Restic ]════════════════╗╭──[ SNAPSHOT_LOG ]──────────╮
║ Repo:     /mnt/backup/restic                                     ║│ #42  2026-05-26 03:00  OK  │
║ Source:   /opt/appdata/*                                         ║│ #41  2026-05-25 03:00  OK  │
║ Schedule: daily @ 03:00                                          ║│ #40  2026-05-24 03:00  OK  │
║ Status:   ✓ IDLE — next run in 21h 14m                           ║│ #39  2026-05-23 03:00  OK  │
║                                                                  ║│ #38  2026-05-22 03:01  OK  │
║ Last backup:   2026-05-26 03:00  ·  4m 32s  ·  14.2 GB          ║│ #37  2026-05-21 03:00  WARN│
║ Total size:    87.4 GB (raw)  →  14.2 GB (dedup, 83.7% savings)  ║│ #36  2026-05-20 03:00  OK  │
║ Snapshots:     42                                                ║╰───────────────────────────╯
║                                                                  ║
║ Stacks paused during backup (API /backup/pause → /backup/resume):║  [b] run backup now
║  ● lxc-media      → paperless, media                            ║  [p] prune old snapshots
║  ● lxc-paperless  → paperless                                    ║  [r] check repo integrity
╚══════════════════════════════════════════════════════════════════╝
```

---

## Color Reference

| Element                        | Color              |
|--------------------------------|--------------------|
| `>> HOST_MESH <<` banner       | Cyan + BOLD        |
| `╔══[...` double border        | Cyan (active)      |
| `╭──[...` rounded border       | Magenta (secondary)|
| `● RUN` / `✓`                  | Green              |
| `○ STP`                        | DarkGray           |
| Selected row                   | Pulsing cyan bg    |
| Warnings / `WARN`              | Yellow             |
| Errors                         | Red                |
| Ticker bar                     | Dark green-gray    |
