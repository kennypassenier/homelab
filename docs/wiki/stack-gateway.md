# Stack: Gateway

> Reverse proxy and security stack. Nginx Proxy Manager handles HTTPS termination; CrowdSec provides active intrusion prevention; GoAccess gives live web analytics.

## Overview

The gateway stack is the public-facing entry point for all homelab services. All apps share a `gateway_network` bridge network, created by `pre-sync.sh` before any app starts.

## Apps

| App | Purpose | Port |
|---|---|---|
| [Nginx Proxy Manager](app-nginx-proxy-manager.md) | HTTPS reverse proxy + Let's Encrypt | 80, 443, 81 (admin) |
| [CrowdSec](app-crowdsec.md) | Intrusion detection + prevention | — |
| [GoAccess](app-goaccess.md) | Web log analytics dashboard | 7880 |
| [Watchtower](app-watchtower.md) | Automatic image updates | — |
| [Promtail](app-promtail.md) | Log shipping to Loki | — |

## `pre-sync.sh`

Creates `gateway_network` idempotently before any compose project is brought up:

```bash
docker network inspect gateway_network >/dev/null 2>&1 || \
  docker network create gateway_network
```

## Network

All apps declare `gateway_network` as an external network:

```yaml
networks:
  gateway_network:
    name: gateway_network
    external: true
```

This allows CrowdSec and GoAccess to read Nginx Proxy Manager's log files (via a shared volume) while staying in separate compose projects.

## See also

- [app-nginx-proxy-manager.md](app-nginx-proxy-manager.md)
- [app-crowdsec.md](app-crowdsec.md)
- [app-goaccess.md](app-goaccess.md)
- [Networking](networking.md)
