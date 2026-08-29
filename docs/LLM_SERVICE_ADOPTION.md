# Adopting an externally-built service into the homelab

Self-contained instructions for an LLM session (or a human) that finds a
service running in a hand-built LXC container and needs the homelab to
take ownership: state recorded, nightly backups, supervised self-updates —
**without restarting the service**. Sister document to
[LLM_COMPOSE_CONVERSION.md](LLM_COMPOSE_CONVERSION.md) (which covers
docker-compose apps; this one covers bare binaries under systemd, C7).

Proven flow: CT 109 (kyu) and CT 112 (almanac) were adopted exactly
this way on 2026-08-29, services untouched throughout.

## What adoption does — and refuses to do

`homelab adopt` verifies the container IS what your stack file claims,
then records it in host state. From that moment the nightly run backs it
up (in-container tar streamed into an encrypted restic repo) and, if an
`update_cmd` is declared, runs the app's own self-update under
supervision: binary preserved first, restart only when the binary actually
changed, health check, rollback from outside the app if the new version
stays down.

It refuses, with a remedy in the message, when:
- the vmid is on the no-touch list, or the live hostname differs from the
  stack file (A1/A2 guards);
- the systemd unit is not `active` — adoption never starts services;
- the unit's real ExecStart does not run the declared binary, or does not
  read the declared EnvironmentFile — fix the stack file to match
  reality, never the other way around;
- any declared path does not exist in the container;
- the stack name already points at a different vmid.

## Step 1 · Make the container conform

Inside the container, the service must look like this (the golden shape —
CT 109 is the reference):

- a **systemd unit** `<name>.service` with `Restart=always` and a short
  `RestartSec`, running as its **own user**, with secrets in an
  `EnvironmentFile` (mode 0640 or tighter, never world-readable);
- the **binary** at a stable absolute path (`/usr/local/bin/<name>` or
  `/opt/<name>/<name>`);
- all mutable state under **declared directories** (e.g.
  `/var/lib/<name>`), not scattered;
- the **hostname** of the container MUST be `<vmid>-app-<stack>` — the
  A2 guard depends on it (`pct set <vmid> --hostname ...` before
  adopting, if needed).

A unit that wraps the binary in `latch run -- <binary>` is fine: the
verification matches the binary anywhere in ExecStart's argv (CT 112
runs exactly that way).

## Step 2 · Write the stack file

`stacks/<stack>/service.yml` in the homelab repo:

```yaml
stack_name: kyu            # [a-z0-9-]
vmid: 109
hostname: 109-app-kyu      # must be <vmid>-app-<stack_name>
unit: kyu                  # systemd unit, no .service suffix
binary: /usr/local/bin/kyu # absolute path
env_file: /etc/kyu/kyu.env   # omit if the unit has none
data_dirs:                     # everything a restore-from-zero needs
  - /var/lib/kyu
  - /etc/kyu
update_cmd: kyu update     # omit = never updated by the homelab
```

Choosing `data_dirs`: ask "if this container burned down, which
directories rebuild the service?" — state AND config. Including
credential material is fine (the restic repo is encrypted; host-meta does
the same). If the binary lives inside a data dir (almanac's
`/opt/almanac`), one entry covers both. Choosing `update_cmd`: run it as
the service's own user when the binary is owned by that user
(`runuser -u <user> -- <binary> update`), plain when root owns it.

## Step 3 · Adopt and prove it

```bash
set -a; . ./.env; set +a
./target/release/homelab adopt stacks/<stack>
homelab backup-native <stack>       # first snapshot, right now
```

Then verify — all four, do not skip any:

1. state: the stack appears in `homelab status` / the TUI with its unit;
2. the service was never touched:
   `pct exec <vmid> -- systemctl is-active <unit>` still `active`, and
   `systemctl show <unit> -p ActiveEnterTimestamp` did not change;
3. the snapshot is real: `restic -r <base>/<stack>-config snapshots`
   shows it, and a restore to a scratch dir lists the expected files
   (a backup that never restored is Schrödinger's backup);
4. nothing readable leaked: no plaintext secrets in the homelab repo,
   argv, or logs.

## Step 4 · What the nightly run does from now on

At the configured hour: tar of `data_dirs` → restic (`<stack>-config`
repo, tiered retention), then `update_cmd` under supervision. A failed
night auto-parks the stack (H8) after one loud message — re-enable with
`homelab enable <stack>` after investigating. `homelab update-native
<stack>` runs the supervised update on demand.

## Known limits

- Restore into a NEW container is manual for now: `pct exec` untar after
  provisioning a base container — the runbook's DR section applies.
- The update supervision's rollback path is mock-proven; the live
  broken-release drill is pending (needs a deliberately broken release,
  coordinated with the service's own project).
