# Stack: Cloudflared

> Cloudflare Tunnel stack. Exposes selected internal services to the internet through an encrypted outbound-only tunnel — no open inbound ports on the router.

## Overview

Cloudflared establishes an outbound-only WireGuard tunnel to Cloudflare's network. Traffic from the public internet hits Cloudflare's edge, travels through the tunnel, and arrives at [Nginx Proxy Manager](app-nginx-proxy-manager.md) without any port-forwarding on the router.

## Apps

| App | Purpose |
|---|---|
| [Cloudflared](app-cloudflared.md) | Cloudflare Tunnel daemon |
| [Watchtower](app-watchtower.md) | Automatic image updates |
| [Promtail](app-promtail.md) | Log shipping to Loki |

## No `pre-sync.sh`

No shared Docker network is needed — cloudflared only needs outbound internet access and the default bridge provides that.

## See also

- [app-cloudflared.md](app-cloudflared.md)
- [stack-gateway.md](stack-gateway.md)
- [networking.md](networking.md)
