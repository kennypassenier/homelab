# bootstrap-lxc.sh

> Bootstraps a new Proxmox LXC container from scratch: installs Docker, clones the Git repo, installs SOPS, decrypts secrets, configures the GitOps cron job, and auto-enables TUN passthrough if the stack needs it.

## Overview

`scripts/host/bootstrap-lxc.sh` is run once per new LXC on the Proxmox host. It fully automates the process of turning a bare LXC into a working GitOps node ready to deploy its assigned stack. It is idempotent in the sense that it guards against re-creating already-present components, but it is best treated as a one-time setup script.

## Usage

```bash
./scripts/host/bootstrap-lxc.sh [-v <VMID>] [-s <STACK_NAME>] [-u <GITHUB_USERNAME>] [-h]
# or via menu:
./host.sh → Bootstrap a new LXC container
```

## Flags

| Flag | Description |
|---|---|
| `-v VMID` | Proxmox VMID of the LXC to bootstrap |
| `-s STACK_NAME` | Stack name to deploy in this LXC |
| `-u GITHUB_USERNAME` | GitHub username for cloning the repo |
| `-h` | Show help and exit |

**Secrets (`GITHUB_PAT`, `AGE_PASSPHRASE`) are intentionally not accepted as CLI flags** — they would be visible to all users in `ps aux`. Provide them via a `.env` file at the repo root or at `scripts/host/.env`, or enter them interactively. The script loads `.env` at startup if it exists.

## Interactive Flow

For any missing value, the script prompts:
1. VMID
2. Stack name (interactive list of all stacks in `stacks/`)
3. GitHub username
4. `GITHUB_PAT` (if not in `.env`) — `read -s` (hidden)
5. `AGE_PASSPHRASE` (if not in `.env`) — `read -s` (hidden)

## Bootstrap Steps (Inside the LXC)

The script uses `pct exec <VMID>` to run commands inside the LXC:

1. **Update packages** — `apt-get update && apt-get upgrade`
2. **Install Docker** — via the official Docker convenience script
3. **Install SOPS** — downloads the v3.9.1 binary
4. **Clone the repository** — sparse checkout of only `stacks/<STACK_NAME>/` and `scripts/` to keep the LXC footprint small, stored at `/opt/gitops`
5. **Configure Git filters** — same smudge/clean filter as on the client
6. **Restore Age key** — decrypts `secrets/age.key.enc` using `AGE_PASSPHRASE` via `openssl`
7. **Run initial node-sync** — triggers the first deployment
8. **Install cron job** — writes `/etc/cron.d/gitops-sync` with a `*/5 * * * *` schedule
9. **TUN auto-detection** — scans the stack's compose files for `/dev/net/tun`; if found, calls [enable-tun.sh](script-enable-tun.md) automatically

## TUN Auto-Detection

Before the LXC starts, the script inspects all `docker-compose.yml` files in `stacks/<STACK_NAME>/` for references to `/dev/net/tun`. If found, TUN passthrough is configured in the LXC's Proxmox config file (`/etc/pve/lxc/<VMID>.conf`) and the LXC is restarted. This is why the [downloader stack](stack-downloader.md) (which uses Gluetun) requires no manual TUN setup.

## Error Handling

A `trap cleanup_on_error EXIT` stops the LXC on unexpected failure and prints troubleshooting tips. It does not attempt to undo partial installations inside the LXC — for a corrupted state, use [reset-stack.sh](script-reset-stack.md).

## Recommended `.env` file (on Proxmox host)

Create `scripts/host/.env` (`chmod 600`):
```bash
VMID=105
STACK_NAME=downloader
GITHUB_USERNAME=myusername
GITHUB_PAT=ghp_xxxxxxxxxxxx
AGE_PASSPHRASE=my-strong-passphrase
```

## See also

- [script-enable-tun.md](script-enable-tun.md)
- [script-reset-stack.md](script-reset-stack.md)
- [script-node-sync.md](script-node-sync.md)
- [Secret Management](secret-management.md)
- [GitOps Flow](gitops-flow.md)
