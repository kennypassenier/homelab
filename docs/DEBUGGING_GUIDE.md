# Debugging guide — when something fails

The system is built to fail loudly, stop early, and leave evidence. This is
the map to that evidence, ordered from "an operation failed" to "the host is
gone" (for the latter, switch to [DR_RUNBOOK.md](DR_RUNBOOK.md)).

**Where to run these.** Anything starting `homelab` runs from anywhere on
Kenny's workstation — the client finds its own configuration. Everything
else here (`journalctl`, `systemctl`, `pct`, `restic`, `curl` at
`127.0.0.1`) is on the **Proxmox host**: `ssh root@10.10.5.250` first, or
wrap it — `ssh root@10.10.5.250 'journalctl -u homelab-host -n 50'`.
Kenny lost a minute on 2026-09-03 to a command from this project that did
not say which machine it belonged to, and a command without its machine is
a command that works for whoever wrote it.

## 1. An operation failed — read the error first

Every failure ends with the same shape:

```
✗ step '<name>' failed :: <why> :: remedy: <what to do>
  :: incident bundle /var/lib/homelab/incidents/<ts>-<op>
```

The `remedy` is written for the situation; start there. `SAFETY ABORT`
means a guard refused on purpose (no-touch vmid, hostname mismatch, shrink
while running, exec disabled) — the fix is your input, not the system.

## 2. Incident bundles (AR14) — the black box

Every failed operation writes `/var/lib/homelab/incidents/<ts>-<op>/`:

| File | What it is |
|---|---|
| `report.json` | the step list with outcomes + the operator error |
| `events.jsonl` | every log/transfer event the operation emitted |
| `commands.sh` | **replayable script of the exact commands run** (AR16) |
| `journal-tail.jsonl` | the operation journal around the failure |
| `versions.txt` | host + proto versions at the time |

Workflow: read `report.json` for *where*, `events.jsonl` for *why*, then
re-run the failing command from `commands.sh` by hand to reproduce.
`homelab incidents` lists bundles. Every reproduced bug becomes a test
before it is fixed (standing rule).

## 3. The journal (B5/AR13) — interrupted operations

`/var/lib/homelab/journal.jsonl` gets a `running` record *before* each step
executes. After a crash or power cut, the daemon logs at boot exactly which
operation was mid-flight ("interrupted operation X at step Y — re-running is
safe") and says so in the `host-online` webhook event (`ok: false`).
Everything is idempotent: the standard recovery is to re-run the operation.

## 4. Daemon-side visibility

```bash
ssh root@10.10.5.250
journalctl -u homelab-host -f            # live daemon log
journalctl -u homelab-host --since -1h   # recent history
systemctl status homelab-host            # watchdog state, restarts
curl -sk https://127.0.0.1:8443/api/health
```

Log verbosity: the daemon honours `RUST_LOG` (AR15) — set
`Environment=RUST_LOG=debug` in the unit for a session, then remove it.
`RUST_LOG=homelab_host=trace` also prints every executor command.

## 5. Common failures → causes

| Symptom | Likely cause / fix |
|---|---|
| `HOMELAB_TOKEN is not set` | source `.env` (`set -a; . ./.env; set +a`) |
| fingerprint mismatch on connect | host cert changed (reinstall/new state dir). If expected: delete `~/.config/homelab/pin` and re-pin; if not expected: investigate before trusting |
| `vmid X is on the no-touch list` | by design — check the manifest's vmid |
| `vmid X has hostname 'Y', expected 'Z'` | manifest points at someone else's container: wrong vmid in the manifest |
| deploy hangs in `wait for systemd` | container has no network (bridge/vlan wrong) or template broken — `pct enter <vmid>` and look |
| `network <stack>_net declared as external…` | compose network name doesn't match the stack name — placeholders wrong in a hand-edited file |
| verify gate: app not running | `homelab exec <vmid> "cd /opt/<stack>/<app> && docker compose logs --tail 50"` (needs exec_enabled) or `pct exec` via ssh |
| backup: `repository … does not exist` | restic repo not initialized for a renamed stack — first backup creates it; check rclone works: `rclone lsd gdrive:homelab-backups` on the host |
| restic `wrong password` | `/var/lib/homelab/secrets/restic.pw` doesn't match the repo — NEVER regenerate over it; restore the real password |
| update reports `ROLLED BACK … now healthy` | the new image is bad; system already recovered — check the app's release notes |
| `daemon-failed` webhook event | daemon crash-looped and systemd gave up: `journalctl -u homelab-host -n 100`, fix, then `systemctl reset-failed homelab-host && systemctl start homelab-host` |
| self-update seemingly ignored | binary failed the 5s health window and was auto-rolled-back — journal shows `homelab-rollback` lines |
| drift flag won't clear | you're comparing different content: run `homelab deploy` (converges + records the new hash) |
| mount changes ignored on redeploy | known edge: mounts/devices are only applied at container CREATE; destroy + redeploy to apply (data in /appdata survives) |

## 6. Verifying the chain end-to-end

```bash
homelab ping      # link + pin + version
homelab doctor    # host self-checks
homelab config    # live host settings
homelab status    # pct list as the host sees it
```

## 7. Where all the state lives (host)

| Path | Contents |
|---|---|
| `/etc/homelab/host.toml` | token, listen, scheduler hour, retention, webhook, exec/mirror/kea config |
| `/var/lib/homelab/state.json` | applied stacks: vmid, apps, hashes, last backup, manifests |
| `/var/lib/homelab/repo` | git history of every deploy's intent (D4) |
| `/var/lib/homelab/secrets/` | env vault + restic password + opnsense creds (0600) |
| `/var/lib/homelab/incidents/` | failure bundles |
| `/var/lib/homelab/journal.jsonl` | operation journal |
| `/var/lib/homelab/audit.log` | every remote-exec invocation |
| `/usr/local/bin/homelab-host{,.prev}` | live + previous binary (H5 rollback) |

## 8. Escalation ladder

1. Re-run the operation (idempotent).
2. Incident bundle → reproduce via `commands.sh`.
3. `journalctl -u homelab-host` + `RUST_LOG=debug`.
4. Fix forward or roll back (git revert in `/var/lib/homelab/repo` +
   redeploy; H5 `.prev` binary for the daemon itself).
5. Host unreachable / hardware gone → [DR_RUNBOOK.md](DR_RUNBOOK.md).
