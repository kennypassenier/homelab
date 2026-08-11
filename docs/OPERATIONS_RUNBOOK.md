# Operations runbook — the recurring work

Day-to-day and periodic operations for a healthy homelab. One-time setup is
in [V2_PILOT_HANDOFF.md](V2_PILOT_HANDOFF.md); disaster recovery is in
[DR_RUNBOOK.md](DR_RUNBOOK.md); failures in
[DEBUGGING_GUIDE.md](DEBUGGING_GUIDE.md).

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
| The daemon itself | build + `homelab self-update target-debian/release/homelab-host` | when we ship changes |
| Golden template | destroy CT 999 → `homelab template-build 999 <v+1>` | after major Debian updates |

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
