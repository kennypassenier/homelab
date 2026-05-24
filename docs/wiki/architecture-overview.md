# Architecture Overview

> Three-tier GitOps homelab: Linux client → Proxmox VE host → LXC containers running Docker.

## Overview

The homelab is built on a strict three-tier architecture. Each tier has its own responsibilities and its own set of scripts — code from one tier is never executed in another.

```
┌─────────────────────────────────────────────────────────┐
│  CLIENT  (Linux Desktop)                                │
│  Git operations, scaffolding, SSH config                │
│  Entry point: client.sh                                 │
└───────────────────────┬─────────────────────────────────┘
                        │  Git push
┌───────────────────────▼─────────────────────────────────┐
│  HOST  (Proxmox VE)                                     │
│  LXC lifecycle, storage, backups, GPU/TUN passthrough   │
│  Entry point: host.sh                                   │
└───────────────────────┬─────────────────────────────────┘
                        │  bind mounts + LXC exec
┌───────────────────────▼─────────────────────────────────┐
│  CONTAINERS  (Unprivileged LXCs)                        │
│  Docker Compose stacks, GitOps sync every 5 min         │
│  Entry point: container.sh                              │
└─────────────────────────────────────────────────────────┘
```

## The Three Tiers

### Client (Linux Desktop)
All day-to-day management happens here. The developer works with Git, creates or removes stacks and apps, manages SSH aliases, and initialises encryption. No Docker or Proxmox tooling is required on the client.

- Scripts live in `scripts/client/`
- Entry point: [`client.sh`](script-client-sh.md) (interactive menu)
- Uses the [lib-ui](lib-ui.md) and [lib-stack](lib-stack.md) shared libraries

### Host (Proxmox VE)
The bare-metal hypervisor runs unprivileged LXC containers. The host is responsible for:
- Bootstrapping new LXCs ([bootstrap-lxc.sh](script-bootstrap-lxc.md))
- Managing NVMe storage mounts (`/opt/appdata/<STACK>`)
- Hardware passthrough — GPU ([enable-gpu.sh](script-enable-gpu.md)) and TUN ([enable-tun.sh](script-enable-tun.md))
- Running Restic backups ([backup-stacks.sh](script-backup-stacks.md))
- Keeping its own local Git clone up to date via [sync-host.sh](script-sync-host.md)

Scripts live in `scripts/host/`. Entry point: [`host.sh`](script-host-sh.md).

### Containers (LXCs)
Each deployed [stack](stack-overview.md) runs in its own LXC. Inside every LXC:
- Docker and Docker Compose are installed
- A Git sparse-checkout of the `stacks/<stack_name>/` folder is maintained at `/opt/gitops`
- [node-sync.sh](script-node-sync.md) runs every 5 minutes via cron to pull changes and deploy them

Scripts live in `scripts/container/`. Entry point: [`container.sh`](script-container-sh.md).

## GitOps Flow

Changes to infrastructure are always made through Git — never by running commands directly inside a container or on the host. See [gitops-flow.md](gitops-flow.md) for a detailed walkthrough.

## Key Concepts

| Concept | Description | Details |
|---|---|---|
| GitOps sync | Automatic pull-and-deploy every 5 min | [gitops-flow.md](gitops-flow.md) |
| Secret management | SOPS + Age transparent encryption | [secret-management.md](secret-management.md) |
| Storage | NVMe host mounts bind-mounted into LXCs | [storage-layout.md](storage-layout.md) |
| Networking | Static IPs via DHCP, SSH aliases | [networking.md](networking.md) |
| Backups | Restic with container pause/resume | [backups.md](backups.md) |

## Automated OS Security Updates (unattended-upgrades)

Alle Debian/Ubuntu-gebaseerde containers ontvangen automatische security updates voor het besturingssysteem. Tijdens het bootstrappen van een nieuwe LXC installeert en configureert het script `bootstrap-lxc.sh` unattended-upgrades, zodat kritieke beveiligingspatches zonder handmatige tussenkomst worden toegepast.

- Dit verkleint het aanvalsoppervlak en houdt het systeem veilig met minimale inspanning.
- De status van unattended-upgrades is te controleren in de container via `systemctl status unattended-upgrades`.

Zie ook: [script-bootstrap-lxc.md](script-bootstrap-lxc.md).

## See also

- [GitOps Flow](gitops-flow.md)
- [Secret Management](secret-management.md)
- [Storage Layout](storage-layout.md)
- [Networking](networking.md)
- [Backups](backups.md)
