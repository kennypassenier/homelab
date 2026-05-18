# App: Nginx Proxy Manager

> HTTPS reverse proxy with a web UI for managing proxy hosts and Let's Encrypt certificates. Front-line entry point for all homelab services.

## Container

| Field | Value |
|---|---|
| Image | `jc21/nginx-proxy-manager:latest` |
| Ports | `80` (HTTP), `443` (HTTPS), `81` (Admin UI) |
| Network | `gateway_network` |

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/gateway/nginx-proxy-manager/data` | `/data` | Proxy host configs, certificates |
| `/appdata/gateway/nginx-proxy-manager/letsencrypt` | `/etc/letsencrypt` | Let's Encrypt data |

The NPM log files in `/appdata/gateway/nginx-proxy-manager/data/logs` are read by both [CrowdSec](app-crowdsec.md) and [GoAccess](app-goaccess.md) via a shared host path.

## CrowdSec Integration

CrowdSec acts as an L7 bouncer against NPM. NPM's access logs are parsed by CrowdSec and banned IPs are blocked at the NPM layer via the CrowdSec bouncer plugin.

## First Run

The default admin credentials on first startup are:
- Email: `admin@example.com`
- Password: `changeme`

Change these immediately after first login.

## See also

- [stack-gateway.md](stack-gateway.md)
- [app-crowdsec.md](app-crowdsec.md)
- [app-goaccess.md](app-goaccess.md)
