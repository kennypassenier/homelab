# The backup model — what was agreed, what was built, what is true

Written 2026-08-30 because a Phase 2 answer and my own earlier claim
contradicted each other. Everything below was read off the code, the Ansible
repo, and the live Google Drive — not remembered.

Kenny's recollection turned out to be accurate on every substantive point.
My earlier statement that "configuration inside a container is invisible to
restic" was true of the Rust orchestrator and false as a general claim: the
Ansible system backed up exactly those in-container paths, and its snapshots
are still on the Drive.

## 1 · The v1 model (Ansible) — what was agreed and built

`~/Projects/ansible/base_tasks/backup.yml`, still on disk.

- restic and rclone were installed **inside every stack container**.
- One repository **per stack**, derived as
  `{{ restic_base_repository }}:{{ stack_name }}-config` with
  `restic_base_repository: "rclone:gdrive"` — so the repo path is
  `gdrive:media-config`, sitting in the **root** of the Drive.
- Inside that repo, the paths backed up are **per service**:
  `/opt/<service>-config`, i.e. the directory each docker-compose file binds
  as its config volume. This is the convention Kenny described.
- **Auto-restore on deploy**, container-agnostic and keyed on the service
  name. Three tasks in sequence: is `/opt/<svc>-config` missing or empty →
  does a snapshot exist for exactly that path (`restic snapshots --path
  /opt/<svc>-config`) → if both, `restic restore latest --target / --path
  /opt/<svc>-config`.

Worked example, media stack: repo `gdrive:media-config` holds snapshots of
`/opt/jellyfin-config`, `/opt/sonarr-config`, `/opt/radarr-config`,
`/opt/bazarr-config`, `/opt/prowlarr-config`, `/opt/seerr-config`. Destroy
CT 106, recreate it, run the play: each of those six directories is checked
individually and refilled from its own snapshot.

## 2 · What is actually on the Drive

Measured 2026-08-30 with `rclone lsf`.

**In the Drive root — the v1 repos.** All seven are genuine restic
repositories (`config`, `data/`, `index/`, `keys/`, `locks/`, `snapshots/`):

| Repo | Snapshots | Newest |
|---|---|---|
| `media-config` | 21 | 2026-07-04 |
| `productivity-config` | 3 | 2026-07-04 |
| `gateway-config` | 3 | 2026-07-04 |
| `cloudflared-config` | 2 | 2026-07-04 |
| `downloader-config` | 2 | 2026-07-04 |
| `monitoring-config` | 0 | — |
| `platform-config` | **0** | **never** |

Two facts fall out of that table:

1. **The v1 backups stopped on 2026-07-04**, eight weeks ago. That is the
   period in which the Ansible repo stopped being able to deploy. So CT 105,
   106 and 111 have a real backup, and it is two months stale.
2. **CT 104 has never been backed up at all.** `platform-config` was
   initialised and never received a snapshot. That container holds Traefik,
   Grafana, Loki, CrowdSec, cloudflared and Uptime Kuma — the entire edge and
   all observability.

**Under `gdrive:homelab-backups/` — the v2 repos**, 76 MiB total:
`almanac-config` (1 snapshot), `mailbox-config` (1), `metrics-config` (1),
`synctest-config` (7), `host-meta-config` (3).

## 3 · The v2 model (Rust orchestrator) — what changed and what did not

**What did not change.** The two ideas Kenny cares about both survived:

- One repository per stack, named `<stack>-config`.
- Auto-restore on deploy, keyed on the stack rather than on the container.
  `core/src/ops/deploy.rs` runs an "auto-restore check" step **before the
  apps start**: if the config directories are empty and a snapshot exists, it
  restores; if the restore fails it warns loudly and continues rather than
  blocking the deploy.

**What changed — three things.**

1. **Where restic runs.** On the Proxmox host, not inside the container
   (`core/src/ops/backup.rs`). It snapshots `manifest.storage[].host_path`.
2. **Where the config lives.** On the host, under `/appdata/<stack>/…`,
   bind-mounted into the container. Example from the live metrics stack:
   ```yaml
   storage:
     - host_path: /appdata/metrics/prometheus-data
       mount_point: /appdata/metrics/prometheus-data
       host_owner_uid: 165534
   ```
   Consequence: a container can be destroyed and recreated without the
   configuration ever being at risk, because it was never inside it.
3. **Where the repo lives.** `rclone:gdrive:homelab-backups/<stack>-config`
   instead of the Drive root. The code says why in a comment: "everything
   lives under one gdrive folder (homelab-backups), not loose dirs in the
   drive root." The seven root directories are the v1 leftovers that
   motivated it.

**Two regressions worth naming, because nothing else names them.**

- **Restore granularity is now all-or-nothing.** v1 checked each service
  directory separately. v2 restores only when **every** path in `storage:`
  is empty (`let mut all_empty = true; … if !all_empty { return Unchanged }`).
  Wipe one app's config while its siblings are intact and the deploy will
  silently not restore it.
- **The path naming drifted.** v1 was strictly `<service>-config`. v2's live
  stacks use `/appdata/metrics/prometheus-data` and
  `/appdata/synctest/synctest-config` — three different shapes for the same
  idea. Nothing enforces a convention.

## 4 · Where each container stands today

| Container | Config lives | Backed up by | Last backup |
|---|---|---|---|
| 104 platform | `/opt/<app>-config` inside | nothing | **never** |
| 105 downloader | `/opt/<app>-config` inside | v1 restic (stopped) | 2026-07-04 |
| 106 media | `/opt/<app>-config` inside | v1 restic (stopped) | 2026-07-04 |
| 111 productivity | `/opt/<app>-config` inside | v1 restic (stopped) | 2026-07-04 |
| 108 synctest | `/appdata/synctest/` on host | v2 restic, nightly | 2026-08-30 |
| 109 kyu | `/var/lib/kyu` inside | vzdump daily + 1 restic snapshot | 2026-08-30 |
| 112 almanac | inside, via C7 pct-exec tar | v2 restic, nightly | 2026-08-30 |
| 113 metrics | `/appdata/metrics/` on host | v2 restic, nightly | 2026-08-30 |

Note the C7 native path (CT 112, and CT 109 once repaired) is a third
mechanism again: `core/src/ops/native.rs` runs
`pct exec <vmid> -- tar -cf - <data_dirs> | restic backup --stdin`, which
*does* reach inside the container. So the orchestrator already has both
techniques; only the docker path is host-only.

## 5 · The open decisions

Put to Kenny in the Phase 2 deep-dive form (items W1-W4):

- **W1** which model the four ansible-era stacks adopt.
- **W2** what happens to the seven v1 repos in the Drive root.
- **W3** whether to restore per-app granularity in the E3 auto-restore.
- **W4** whether `<app>-config` becomes an enforced naming rule.
