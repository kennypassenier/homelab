# App: Cloudflared

> Cloudflare Tunnel daemon. Opens an outbound-only encrypted tunnel to Cloudflare's edge, making internal services publicly accessible without port-forwarding.

## Container

| Field | Value |
|---|---|
| Image | `cloudflare/cloudflared:latest` |
| Command | `tunnel --no-autoupdate run --token ${TUNNEL_TOKEN}` |

`--no-autoupdate` disables cloudflared's built-in self-updater; [Watchtower](app-watchtower.md) handles image updates instead.

## Environment Variables

| Variable | Source | Description |
|---|---|---|
| `TUNNEL_TOKEN` | `.env` (SOPS) | Cloudflare Tunnel token — encrypted in Git |

## Tunnel Configuration

The tunnel routes are configured in the Cloudflare dashboard (Zero Trust → Tunnels), not in this repo. The token identifies which tunnel configuration to load.

## See also

- [stack-cloudflared.md](stack-cloudflared.md)
- [secret-management.md](secret-management.md)
- [app-nginx-proxy-manager.md](app-nginx-proxy-manager.md)
