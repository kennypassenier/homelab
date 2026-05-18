# App: GoAccess

> Real-time web log analytics. Reads Nginx Proxy Manager access logs and presents a live dashboard.

## Container

| Field | Value |
|---|---|
| Image | `xavierh/goaccess-for-nginxproxymanager:latest` |
| Port | `7880` |
| Network | `gateway_network` |

GoAccess is a pre-built image that knows how to parse NPM's combined log format. The dashboard updates in real-time via a WebSocket.

**No authentication is built in.** Access should be restricted at the Nginx Proxy Manager level (e.g., an auth proxy or IP allowlist).

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/gateway/nginx-proxy-manager/data/logs` | `/opt/log` (ro) | NPM access logs |

## See also

- [stack-gateway.md](stack-gateway.md)
- [app-nginx-proxy-manager.md](app-nginx-proxy-manager.md)
