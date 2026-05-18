# Stack: Downloader

> VPN-protected BitTorrent download stack. All traffic from qBittorrent is forced through a Gluetun WireGuard tunnel — the container cannot reach the internet if the VPN is down.

## Overview

The downloader stack runs in a dedicated LXC and provides a VPN kill-switched download pipeline. It requires [TUN passthrough](script-enable-tun.md) on the LXC because Gluetun creates a virtual TUN network interface inside Docker.

## Apps

| App | Purpose | Port |
|---|---|---|
| [qBittorrent + Gluetun](app-qbittorrent.md) | BitTorrent client behind VPN kill switch | 8080 (Web UI) |
| [Watchtower](app-watchtower.md) | Automatic image updates | — |
| [Promtail](app-promtail.md) | Log shipping to Loki | — |

## Architecture

```
Internet
   │
   ▼ WireGuard (Surfshark)
┌──────────────────────┐
│  gluetun container   │
│  health: port 9999   │
└──────────┬───────────┘
           │  network_mode: service:gluetun
┌──────────▼───────────┐
│  qbittorrent         │
│  Web UI: port 8080   │
└──────────────────────┘
```

qBittorrent has **no network stack of its own** — it uses `network_mode: service:gluetun`, which means all its traffic goes through Gluetun's network namespace. If Gluetun stops, qBittorrent loses all connectivity immediately (the kill switch).

qBittorrent will not start until Gluetun's health check passes (`condition: service_healthy`). The health check polls Gluetun's internal HTTP server on port 9999.

## Key Notes

- **No `pre-sync.sh`** — unlike media and gateway, the downloader stack does not need an external Docker network
- **TUN passthrough required** — configured automatically by [bootstrap-lxc.sh](script-bootstrap-lxc.md) or retroactively via [enable-tun.sh](script-enable-tun.md)
- **Downloads mount** — `/mnt/downloads` on the Proxmox host is bind-mounted into the LXC and used by qBittorrent
- **VueTorrent UI** — installed on every container start via the LSIO Docker mod; activate once in qBittorrent settings: `Settings → Web UI → Use alternative Web UI → /vuetorrent`

## Storage

| Host path | Container path | Purpose |
|---|---|---|
| `/opt/appdata/downloader/qbittorrent/config` | `/config` | qBittorrent configuration |
| `/mnt/downloads` | `/downloads` | Download target directory |

## See also

- [app-qbittorrent.md](app-qbittorrent.md)
- [app-watchtower.md](app-watchtower.md)
- [app-promtail.md](app-promtail.md)
- [script-enable-tun.md](script-enable-tun.md)
- [Networking](networking.md)
