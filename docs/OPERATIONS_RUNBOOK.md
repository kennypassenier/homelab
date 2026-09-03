# Operations runbook — the recurring work

Day-to-day and periodic operations for a healthy homelab. One-time setup is
in [V2_PILOT_HANDOFF.md](V2_PILOT_HANDOFF.md); disaster recovery is in
[DR_RUNBOOK.md](DR_RUNBOOK.md); failures in
[DEBUGGING_GUIDE.md](DEBUGGING_GUIDE.md).

**Where to run these.** `homelab …` runs from anywhere on the workstation.
Anything else — `restic`, `rclone`, `pct`, `systemctl`, `journalctl` — is on
the **Proxmox host** (`ssh root@10.10.5.250`), because that is where the
daemon, the vault and the containers are.

## Routine: nothing (that's the point)

The nightly scheduler (04:00, adjustable in SETTINGS) backs up every managed
stack and updates `auto`-policy apps. Failures announce themselves through
the HA webhook into `/media/homelab_events.log` (and as notifications once
the toggle is on). Unattended-upgrades patches security updates inside every
container daily. You only act when an event says so.

## Weekly-ish glance

```bash
homelab doctor                  # host self-checks green?
homelab tui                     # dashboard: drift flags? apps down? capacity?
```
Check the events log in HA (Media → local → homelab_events.log) for
anything `ok:false` you missed.

## Adding a service

1. Preset exists? `homelab presets`. If not: [PRESET_GUIDE.md](PRESET_GUIDE.md)
   (vendor compose? feed it + [LLM_COMPOSE_CONVERSION.md](LLM_COMPOSE_CONVERSION.md)
   to an LLM).
2. TUI `N` → wizard → scaffold.
3. Secrets? `stacks/<name>/<app>/.env` first.
4. `P` to preview, `SHIFT+D` (or `homelab deploy stacks/<name>`).
5. First backup: `homelab backup stacks/<name>` (creates the repo).
6. Commit the stack dir to git.

## Updating things

| What | How | Cadence |
|---|---|---|
| One app | `homelab update stacks/<name> <app>` | when release notes please you |
| App automatically | label `com.homelab.update.policy=auto` | nightly |
| Container OS (security) | unattended-upgrades | automatic daily |
| Container OS (full) | `homelab patch` | monthly-ish |
| The daemon itself | `make release VERSION=x.y.z` then `homelab release-update` — see below | when we ship changes |
| Golden template | destroy CT 999 → `homelab template-build 999 <v+1>` | after major Debian updates |

## Making a release (H7 flow)
*(H7 = release-driven host updates: the TUI spots a newer GitHub release
and installs it over the line on a keypress.)*

Working on the code itself — gates, commit hooks, the full release
walkthrough — is [DEVELOPMENT.md](DEVELOPMENT.md). One line matters even if
you never touch the code: **after cloning this repo on any machine, run
`make hooks` once**, or commits are accepted with failing tests.


The normal path for shipping daemon changes, end to end:

1. Land your changes on `v2-merge` with the gates green (`make gate`).
2. `make release VERSION=x.y.z` — runs the full gate locally, stamps the
   workspace version, commits, tags `vx.y.z` and pushes. Refuses on a dirty
   tree or an existing tag. Version rule: breaking/architectural = major,
   feature = minor, fix = patch.
3. GitHub CI re-runs the gate (a red gate blocks the release) and publishes
   `homelab-host`, `homelab` and `SHA256SUMS` as a GitHub Release.
   Watch with `gh run watch`.
4. Publishing changes nothing on the host. Roll out deliberately:
   the TUI shows "⬆ HOST UPDATE vx.y.z" — press `U`; or run
   `homelab release-update`. The client downloads the release, verifies the
   checksum, and ships it over the line into the existing self-update
   pipeline (selfcheck → backup → armed rollback → restart).
5. If the new daemon crashes on start, systemd rolls back to the previous
   binary automatically — nothing to do but read the incident.

Emergency path without GitHub: `make host-binary` +
`homelab self-update target-debian/release/homelab-host`.

## Parking a service (H8)

`homelab disable <stack>` (or `E` on the stack in the TUI) parks a stack:
nightly backup+update runs skip it and onboot is cleared, so it stays down
across host reboots. Containers are NOT stopped — do that manually if you
want it down now (`pct stop` in Proxmox is always respected; the flag never
fights you). `homelab enable <stack>` reverses both. A failed nightly run
auto-parks the stack after one loud message so it cannot fail every night;
investigate, then re-enable.

## The host's own crown jewels (H10)

Every nightly run ends with a `host-meta` snapshot: the secrets vault
(including `restic.pw` — the key to EVERY other backup), `state.json`, the
TLS certificate + key, and the intent repo with its full deploy history.
On demand: `homelab backup-host-meta`.

**Exact recovery path after losing the host disk** (write this down offline —
you cannot read it from the machine that died):

```
restic -r rclone:gdrive:homelab-backups/host-meta-config restore latest --target /
```

Note the repo is `host-meta-config` — every repo carries the `-config`
suffix. It is encrypted with the restic password that lives INSIDE it, so
an offline copy of `/var/lib/homelab/secrets/restic.pw` (password manager,
second machine) is what makes this recoverable at all. Without it the
backups are unopenable — no exception, no recovery service.

## ZFS snapshots + replication (E8)

Declared in `/etc/homelab/host.toml`:

```toml
[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[[zfs_jobs]]
source = "HDD4TB"
target = "HDD18TB/replica/HDD4TB"
```

The retired cron script replicated into `HDD18TB/REPLICA_2TB` and
`REPLICA_4TB`. Those datasets are LEFT ALONE as frozen history (53
snapshots, May–August 2026): its retention pruned parents and children on
different schedules, so that subtree can no longer accept an incremental
stream. Nothing had to be destroyed — the new chain simply lives next to it.
Delete the old datasets whenever you are comfortable:
`zfs destroy -r HDD18TB/REPLICA_2TB` (and `_4TB`).

Runs at the end of every nightly run and on demand with
`homelab zfs-replicate`. Snapshots are named `homelab-YYYYMMDD-HHMM`; the
old `backup-*` snapshots from the retired cron script are left untouched.

**When it refuses**: "share no snapshot, but the target already holds N
snapshots". That means the incremental chain broke (a snapshot was deleted,
or a pool was re-created). Re-seeding would destroy the replica's history,
so it stops. Investigate first; if a fresh seed really is what you want,
`zfs destroy -r <target>` yourself and re-run. This refusal is the whole
reason the feature exists — the script it replaces destroyed and re-sent
automatically, which is one bad night away from losing every replica.

**Mail vs webhook**: the retired script mailed its own HTML report; that mail
is gone with it. E8 reports the way everything else does — the Home Assistant
webhook and, on failure, an incident bundle. Proxmox keeps sending its own
mails (vzdump, cluster alerts) through `/etc/pve/notifications.cfg` →
`mail-to-root` → the GMail SMTP target; that is Proxmox's own channel and is
untouched by the homelab.

**Media is deliberately out of scope**: HDD12TB and the 18TB data are films
and series — re-downloadable, and there is no room to replicate them.

## Backup verification (quarterly drill)

```bash
homelab restore stacks/<name>       # latest snapshot, full verify chain
```
Do it on a low-stakes stack. A backup that has never been restored is a
hope, not a backup. Also verify the Drive side once in a while:
`rclone lsd gdrive:homelab-backups` on the host.

**Keep an offline copy of `/var/lib/homelab/secrets/restic.pw`** — without
it every backup, old and new, is unreadable.

## Resource changes

Edit the manifest, then `homelab resize stacks/<name>` (live grow). Shrink:
stop the container first, or let the next destroy+deploy apply it.

## Removing a service

```bash
homelab destroy stacks/<name>    # typed-name confirm; /appdata survives
```
Data cleanup afterwards is deliberate and manual: the `/appdata/<stack>/`
dir on the host, and `rclone purge gdrive:homelab-backups/<stack>-config`
once you're sure. Remove the stack dir from git last.

## After a power cut

Nothing to do: containers come back per boot order, the daemon announces
`host-online` to HA with any interrupted operation named; interrupted
operations are safe to re-run. If the daemon itself doesn't come back:
DEBUGGING_GUIDE §5 (`daemon-failed`).

## Certificates, tokens, credentials inventory

| Credential | Lives | Rotate/renew |
|---|---|---|
| API bearer token | `.env` (client) + host.toml | rotate by editing both |
| TLS cert + pin | `/var/lib/homelab` + `~/.config/homelab/pin` | regenerate = delete cert files, restart daemon, re-pin |
| restic password | host secrets + **offline copy** | never rotate lightly (old repos!) |
| Google Drive OAuth | host rclone.conf (own client) | re-auth: `rclone authorize` flow |
| OPNsense API (H2) | `/var/lib/homelab/secrets/opnsense` | OPNsense → Access → Users |
| PVE metrics token (F4) | metrics stack `.env` | Proxmox → API tokens |
| App secrets | host vault via stack `.env`s | redeploy after editing |

## Standing rules

- Red CI blocks merge; every bug becomes a test; docs update with the
  milestone that changes them.
- The Proxmox host is never touched outside an agreed step.
- The no-touch list is code, not convention — extend it in
  `core/src/safety.rs` when new unmanaged guests appear.
- vmid 108 stays the automated-test container until Kenny reassigns it.
