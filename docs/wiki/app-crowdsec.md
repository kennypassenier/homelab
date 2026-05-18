# App: CrowdSec

> Collaborative intrusion prevention system. Parses Nginx Proxy Manager access logs, detects attacks using community threat intelligence, and blocks offending IPs.

## Container

| Field | Value |
|---|---|
| Image | `crowdsecurity/crowdsec:latest` |
| Network | `gateway_network` |
| Collection | `crowdsecurity/nginx-proxy-manager` |

## Log Parsing

CrowdSec reads NPM access logs from the host path:
```
/appdata/gateway/nginx-proxy-manager/data/logs
```
This is mounted read-only into the CrowdSec container.

## Whitelist

`stacks/gateway/crowdsec/whitelists.yaml` is mounted to:
```
/etc/crowdsec/parsers/s02-enrich/whitelists.yaml
```

It whitelists LAN IP ranges and Tailscale addresses so internal traffic is never flagged. Edit the file in Git and let [node-sync.sh](script-node-sync.md) apply it — the file is bind-mounted directly, so CrowdSec picks up changes without a restart.

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/gateway/crowdsec/data` | `/var/lib/crowdsec/data` | CrowdSec decisions + state |
| `/appdata/gateway/crowdsec/config` | `/etc/crowdsec` | CrowdSec configuration |
| `/appdata/gateway/nginx-proxy-manager/data/logs` | `/var/log/nginx` (ro) | NPM access logs for parsing |
| `./whitelists.yaml` | `/etc/crowdsec/parsers/s02-enrich/whitelists.yaml` | IP whitelist |

## See also

- [stack-gateway.md](stack-gateway.md)
- [app-nginx-proxy-manager.md](app-nginx-proxy-manager.md)
