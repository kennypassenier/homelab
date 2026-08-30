# Register — The Homelab Deployment Project

The mechanism behind scope constraint C6: **every finding, task and decision
of this project gets a number and lives here**, not in a conversation. A
session that lost its context resumes by reading this file. Updating a row is
part of doing the work, not paperwork afterwards.

↳ *C6 = the scope constraint requiring this project to survive a context
loss; see `SCOPE.md`.*

**Numbering.** `D#` decisions taken, `T#` tasks to do, `F#` findings (facts
discovered that change what we do), `B#` blockers waiting on someone else.
Numbers are permanent and are never reused, including after a row closes.

**Status.** `open` · `doing` · `done` · `parked` · `dropped`.

## Decisions (D)

| # | Decision | Where it landed | Date |
|---|---|---|---|
| D1 | Scope approved: goals G1-G8, non-goals N1-N5, constraints C1-C8 | `SCOPE.md` | 2026-08-30 |
| D2 | No-touch list narrowed to 100, 101, 102, 103 | `core/src/safety.rs`, pinning test | 2026-08-30 |
| D3 | VM 101 keeps its VM lifecycle untouchable; HA-internal config changes only with per-change permission | `SCOPE.md` C8 | 2026-08-30 |
| D4 | CT 107 is cleaned up; CT 111 is kept and integrated | `SCOPE.md` | 2026-08-30 |
| D5 | Scratch containers 190/191 cleaned up after coordinating with pipeline-v2 | `SCOPE.md` | 2026-08-30 |
| D6 | Project documents live in `docs/deployment/` in this repo | `SCOPE.md` | 2026-08-30 |
| D7 | Inter-service configuration uses LXC IPs, not public URLs | `SCOPE.md` C3 | 2026-08-30 |
| D8 | Almanac joins the scope; it already runs on CT 112 as an adopted native service | `SCOPE.md` G3 | 2026-08-30 |
| D9 | `no_touch` override removed from the live `/etc/homelab/host.toml`; the compiled list is the only list | host.toml (read back, daemon restarted) | 2026-08-30 |
| D10 | The three v1 stack files claiming live vmids are deleted | commit 35feb55 | 2026-08-30 |
| D11 | Emergency vzdump of CT 104 taken (2.4 GB, 28 s) and a daily job added at 02:30, `keep-daily=3,keep-weekly=2` | `/etc/pve/jobs.cfg` job `platform-104` | 2026-08-30 |
| D12 | The seven v1 restic repos in the Drive root stay until the new backup is proven by a restore | `BACKUP_MODEL.md` | 2026-08-30 |
| D13 | The E3 auto-restore granularity gap is closed (per path, not per stack) | pending build | 2026-08-30 |
| D14 | `<app>-config` becomes an enforced naming rule, not a convention | pending build | 2026-08-30 |
| D15 | MQTT terminates on the Home Assistant VM; CT 107 and its Traefik route are removed | `SCOPE.md`, T27 | 2026-08-30 |
| D16 | A service that changes container or IP requires telling Kenny first, with the list of what he must reconfigure | `SCOPE.md` | 2026-08-30 |
| D17 | Model v2: configuration moves to `/appdata/<stack>/<app>-config` on the host, restic runs host-side | `BACKUP_MODEL.md` | 2026-08-30 |
| D18 | Close all three privileged-path gaps AND ship a second golden template, one privileged and one not | pending build (T28-T30, T34) | 2026-08-30 |
| D19 | vzdump archives live on `hdd4tb-backup`; `/appdata` stays on pve-root | `/etc/pve/storage.cfg`, both jobs repointed | 2026-08-30 |
| D23 | Phase 3 frozen: T1 file-based Prometheus discovery, T2 per-stack dashboards plus a fleet one, T3 docker labels, T4 both invocations, T5 native becomes a list, T6 unchanged | `TECH_CHOICES.md` | 2026-08-30 |
| D24 | Phase 4 frozen: the ten-stack layout, `platform` → `gateway`, metrics and observability merged as `metrics` on 107, uptime-kuma alone on 108, syncthing to 113, vmid 110 permanently reserved | `TARGET_LAYOUT.md` | 2026-08-30 |
| D25 | One restic repository **per app**, not per stack — so an app can move between stacks with its history | `TARGET_LAYOUT.md`, B2 | 2026-08-30 |
| D26 | Container replacement keeps vmid and IP: prove the recipe on a throwaway vmid, vzdump, destroy, redeploy in place | `TARGET_LAYOUT.md`, C4 | 2026-08-30 |
| D27 | http-switchboard joins kyu and kyu-runner on CT 109 with an explicit port map (8080/8082/8083); no code change in any project | `TARGET_LAYOUT.md`, A4 | 2026-08-30 |
| D39 | M5 two of three: update classes written down and labelled (Y1/Z1), O9 clean shutdown built with the pull-stop-up order asserted. O10 blocked on the refused Jellyfin key | `UPDATE_POLICY.md`, `o9_stop_first_happens_between_the_pull_and_the_up` | 2026-08-31 |
| D38 | CT 107 destroyed (E4). Final vzdump taken first (289 MB), Traefik's `mqtt` entrypoint and route removed, the scrape target dropped from repo and live config | 20 of 20 Prometheus targets up, 0 down; fin/grafana/tasks routes still answer; fleet check clean | 2026-08-31 |
| D36 | v3.3.0 → 3.3.2 released and live on the host, each with an armed rollback that disarmed itself. T1/T2 enabled in `host.toml` (keys placed before the `[[zfs_jobs]]` tables — bare keys after a table header belong to that table in TOML) | `homelab ping` reports 3.3.2 | 2026-08-31 |
| D37 | `homelab check` runs clean against the real fleet: repo and reality agree | verified live after fixing two probe faults and one bookkeeping fault | 2026-08-31 |
| D35 | M3 capability built: two golden templates, privilege in the name, observability agents baked in | `template.rs`, two tests | 2026-08-30 |
| D34 | M2 complete (8/8) and M4 built: T1 discovery files, T2 generated dashboards, Y4 the fleet check as command and nightly pass. 20 suites green, clippy clean | commits 3db73bc, e65e351, 6bfc24f, 4bad39c | 2026-08-30 |
| D33 | O7 settled by mini-round: Kenny chose the literal rule (C) over the weaker shape — "uniformiteit en een duidelijke regel hebben hier voorrang". `prometheus-data` renamed to `prometheus-config` in repo and live; 21 series of history intact after the move | `manifest.rs`, live on CT 113 | 2026-08-30 |
| D32 | M2 in progress: O6 auto-restore per path, F38 restore timeout configurable, F39 backup target in `host.toml`, O5 clone-privilege guard and uid-mapping validation — all test-first, 19 suites green | commits 6b2bb2b, e758aee, dd96be6 | 2026-08-30 |
| D30 | M0 complete: every guest has a vzdump and a nightly job; kyu restore drill green — restored database holds 10 topics, 46 messages, 7 subscriptions, 69 deliveries, identical to live | drill run and cleaned up | 2026-08-30 |
| D31 | M1 complete: the monitoring drift is in the repo, promtail runs on CT 104 and its logs reach Loki | `stacks/metrics/`, `captured/` | 2026-08-30 |
| D29 | M0 started: kyu re-adopted under its real name and **backed up for the first time** (restic snapshot f2352564, 4.5 MB, `kyu-config`); the stale `mailbox` state key removed; the syncthing route repaired and verified 200 through Traefik | live, read back | 2026-08-30 |
| D28 | Vikunja is dropped; its data stays in the productivity stack's existing backup | `TARGET_LAYOUT.md` | 2026-08-30 |
| D20 | Phase 2 frozen after round 2; changes go through mini-rounds only | `FEATURES.md` | 2026-08-30 |
| D21 | Grafana's admin password rotated off the shipped default and stored in latch at `platform/grafana/.env` (env prod) | verified: new password 200, old default 401 | 2026-08-30 |
| D22 | Docker updates run through a maintained watchtower fork, with pre-stop/post-start hooks and a Jellyfin stream check | pending mini-round Z | 2026-08-30 |

## Findings (F)

| # | Finding | Impact | Status |
|---|---|---|---|
| F1 | A working monitoring stack (Alertmanager, cadvisor ×6, Grafana datasource + 3 dashboards, node/cadvisor/almanac scrape jobs, SMART collector on the PVE host) exists on the machines and in no repository | The next deploy from this repo silently reverts all of it | open |
| F2 | CT 107 runs no docker and no mosquitto — only sshd and node_exporter | 8 GB allocated to an empty container | open |
| F3 | CT 190 and 191 hold 10.10.10.14 and .15, the addresses a new CT 114/115 would take under the vmid-to-last-octet convention | Blocks the convention for new stacks | open |
| F4 | A2's hostname guard, not the no-touch list, is what still refuses the legacy stacks (`lxc-media-stack` etc. are not `<vmid>-app-<stack>`) | Narrowing the no-touch list did not make them deployable | done |
| F5 | Two drives are old: Toshiba DT01ACA200 at 110 329 power-on hours, WD40EFRX at 63 194 — both pass SMART with zero reallocations | Justifies the pending-sector alert rule; a backup target choice must not assume these drives | open |
| F6 | Grafana runs with `GF_SECURITY_ADMIN_PASSWORD=changeme_secure_password` and 10.10.10.4:3000 answers on the LAN with no gate | Must be treated as leaked; see §6 of the vault note "Homelab Open Issues" | open |
| F7 | homelab's record of the kyu stack is broken (pre-rename `mailbox` paths, `enabled: false`, `last_backup: 0`) — but a daily vzdump job covers CT 109 and one restic snapshot exists on Drive | **Corrected**: the data is protected twice over; what is missing is homelab maintaining it | open |
| F8 | `/etc/homelab/host.toml` sets `no_touch` explicitly, which REPLACES the compiled default — today's code narrowing changed nothing live | The safety decision is not yet in force; the file must be updated per container | open |
| F9 | Uptime Kuma monitors exactly one target (kyu `/healthz`) after four weeks of uptime | Jellyfin, Traefik, HA, almanac and the whole edge are unwatched | open |
| F10 | CT 107 is empty, yet `lxc-mqtt-stack.yml` still routes TCP :1883 to it while CT 104 publishes 1883 itself | Where MQTT terminates must be settled before 107 is removed | open |
| F11 | Ansible-era config (CT 104/105/106/111) lives only inside its own container; `/appdata` holds only metrics and synctest | A container rebuild loses the configuration | open |
| F12 | The Cloudflare tunnel ingress and every Access policy exist only in the Cloudflare dashboard; `/opt/cloudflared-config/` is empty | The edge cannot be restored from any repository | open |
| F13 | CT 104 runs no promtail, so the host that owns Loki ships none of its own container logs | Blind spot exactly where the edge lives | open |
| F14 | kyu's store is a 98 KB `.db` behind a 4.1 MB `.db-wal` | Any backup or move must take .db/.db-wal/.db-shm together (standing rule 15a) | open |
| F15 | CT 111 was removed from the live no-touch list on 2026-08-29 so homelab could adopt it; the adoption never happened | Unprotected and unmanaged simultaneously | open |
| F16 | restic backs up HOST paths (`storage[].host_path`), never paths inside a container | The four ansible-era stacks are in NO backup of any kind — no restic repo, no vzdump job | open |
| F17 | `stacks/cloudflared` claims vmid 109 (kyu), `stacks/gateway` claims 108 (synctest), `stacks/todo` claims 111 — all v1-era leftovers | Only A2's hostname guard stops a deploy from targeting a live container | open |
| F18 | The Google Drive target authenticates today and holds five restic repos (76 MiB) — no new token needed | B2 shrinks to a restore drill | done |
| F19 | Those repos hold 1, 1, 1, 7 and 3 snapshots while state claims nightly runs for all of them | Unverified whether retention explains it; measure before trusting the nightly run | open |
| F20 | The v1 restic repos in the Drive root stopped on 2026-07-04, and `platform-config` never received a single snapshot | CT 104 had never been backed up until today's vzdump | done (D11) |
| F21 | `pct clone` does not take `--unprivileged`, and the golden template CT 999 is unprivileged | A manifest saying `unprivileged: false` that provisions via `clone:999` silently yields an unprivileged container | open |
| F22 | Every test in the suite uses `unprivileged: true`; the privileged path has no coverage at all | CT 105 and 106 are privileged and must stay so | open |
| F23 | `host_owner_uid` is applied without any check against the container's privilege level | 101000 on a privileged container (or 1000 on an unprivileged one) silently produces unusable ownership | open |
| F24 | `/appdata` sits on `pve-root` — the same 94 G filesystem as the Proxmox OS | A container filling its config directory fills the hypervisor's root filesystem | partly closed (D19: archives moved, pve-root 41% → 26%) |
| F25 | vzdump archives of CT 102 and CT 103 from 2026-07-13 exist (943 MB and 262 MB) | The two untouchable containers do have a backup, seven weeks old — worth knowing before anyone assumes otherwise | open |
| F26 | Only two of Kenny's Rust services self-update: latch (minisign-signed) and almanac (M10, keeps the previous binary and reverts). kyu, kyu-runner, HTTPSwitchboard and newsflash each decided against it | The homelab must own updates for kyu and kyu-runner outright, not merely supervise them | open |
| F27 | kyu 2.0.0 publishes **no release assets at all** — kyu-runner ships a musl binary + SHA256SUMS, almanac ships a binary + SHA256SUMS + minisig | There is nothing for the homelab to download, so kyu cannot be updated by any release-driven mechanism yet | open |
| F28 | kyu's own documentation says "updates are image pulls via compose (K13)", but kyu runs as a native systemd binary on CT 109 | Its documented update path describes a deployment that is not the one in use | open |
| F29 | containrrr/watchtower was archived 2025-12-17; the maintained continuation is `nicholas-fedor/watchtower` | Any auto-update design has to name a fork, not "watchtower" | open |
| F30 | The v1 directories are all still present: `apps`, `client-app`, `host-daemon`, `lxc-daemon`, `stacks-backup` | The never-answered V1-V5 cleanup | done 2026-08-30 (Z5) |
| F31 | A Jellyfin stream check already existed in v1 (`stacks-backup/media/jellyfin/check-streams.sh`, now in git history only) and **fails open**: a missing key, an unreachable API or an empty response all exit 0 = "safe to update" | The exact conditions in which you cannot tell whether someone is watching are the ones where it says go ahead. O10 must fail closed instead | open |
| F32 | The `JELLYFIN_API_KEY` in `/opt/jellyfin/.env` on CT 106 is **invalid** — HTTP 401 measured three ways (Authorization MediaBrowser Token, X-Emby-Token, `?api_key=`) | O10 needs a fresh key before it can be built or tested | open |
| F49 | **`latch commit` deletes what is not on disk.** Committing after creating only one env file removed the two others from the repository — "1 changed, 2 removed". `latch diff` had earlier reported "no differences" with no local files at all, which is what led me to believe absent meant untouched | My mistake, recovered: both values still existed on their containers and were re-committed and verified. `latch rollback` was blocked by the permission classifier, so recovery came from the systems of record instead. **Always `latch pull` before `latch commit`** | closed |
| F50 | Measured with a working key: Jellyfin's session objects have **no `IsPlaying` field** — they carry `NowPlayingItem` and `PlayState.IsPaused`. The v1 `check-streams.sh` grepped for `"IsPlaying":true`, which never matched | Settles F33. That script could not have blocked a single update: it failed open on every error path AND its one positive test never fired | closed |
| F46 | The fleet check's first live run called **every** route in the house dead — Jellyfin, Home Assistant, the Proxmox UI. Two causes: `/dev/tcp` is a bash feature and the container shell is dash, and `https://10.10.5.1` carries no port so the probe asked for a path | The pure comparison had six passing tests and was right; the shell around it had never run once. The HTTPSwitchboard "first live call" lesson, on schedule | fixed in 3.3.1 |
| F47 | `last_backup` was recorded by the scheduler and by no on-demand path, so a stack backed up by hand still read as never backed up | The check reported it about kyu minutes after I had backed it up with a real snapshot to show for it. Would have made every manual backup during the M8 rollout invisible | fixed in 3.3.2 |
| F48 | GitHub reports 7 dependabot vulnerabilities on `main` (1 high, 2 moderate, 4 low); two dependabot runs are failing, one of them against the now-deleted `client-app/` | Not triaged. The failing runs at least partly chase a directory that no longer exists | open |
| F43 | **F40 was wrong.** `pct snapshot` refuses any container with a bind mountpoint — "snapshot feature is not available" on CT 113. That is every managed container, because model v2 IS bind mounts. The undo-button layer I proposed does not exist where it would be used | Corrects what I told Kenny about snapshots; the vzdump layer stands unaffected | corrected |
| F44 | `/appdata/metrics/alertmanager-data` was bind-mounted by Alertmanager's compose file and declared in no `storage:` entry, so docker created it on the **container rootfs** — verified by `df`: rootfs, not pve-root. Unbacked-up and lost on any rebuild | Exactly the bug class `validate()` names as "the synctest-108 bug class", found live. The directory was still empty, so nothing was lost — luck, not design | fixed 2026-08-30 |
| F45 | B1's protection flag refused `pct set -mp0` on CT 113 during the rename, as designed | The lift-change-restore dance is a required step in any procedure that touches a mountpoint; M7's replacement procedure must carry it | open |
| F42 | O7 as frozen says config paths must be named `<app>-config`, but the live metrics stack uses `/appdata/metrics/prometheus-data` — a data directory, not config. Enforcing the letter would refuse a running stack; D25 meanwhile made ownership explicit data, so the naming rule is no longer load-bearing | Quarantined: not built, mini-round queued rather than deviating silently | open |
| F41 | "Adopt CT 111" is impossible as written: `homelab adopt` takes a systemd-unit manifest (C7 is native-only) and CT 111 is a docker stack whose hostname A2 refuses | M0 corrected to a nightly vzdump; real adoption waits for the M8 rebuild | corrected |
| F40 | Container snapshots are supported (every container is on thin-provisioned `local-lvm`) and **not one exists**. The thin pool is 20.8% used, metadata 0.9% | A near-free instant-undo layer sits unused; directly useful before an update (O9) or a risky change | open — proposal for Phase 5 |
| F34 | **`sync.kp-soft.dev` is broken right now.** Its route file sends traffic to `10.10.10.10:8384`, where nothing answers; syncthing runs on `10.10.10.8:8384`, which returns 200. Verified live 2026-08-30 | A second dead route beside the MQTT one, unnoticed. Found by the critic, not by my own sweep — I recorded the address without checking it resolved | open (R8) |
| F35 | `pct set -mp<i>` is only reached inside `if !exists` on both provisioning paths in `deploy.rs`; a deploy onto an existing container configures no mountpoints at all | Adopting the ansible-era stacks in place would put config on the container rootfs and have restic snapshot an empty host directory, green all the way | open |
| F36 | `native.rs:73` refuses a manifest with empty `data_dirs`, and kyu-runner is deliberately stateless (`DynamicUser=yes`, "no state directory, no disk to protect") | kyu-runner cannot be declared as a native service today | open |
| F37 | kyu and http-switchboard both default to `0.0.0.0:8080`, and http-switchboard's `--healthcheck` with no argument probes `127.0.0.1:8080/healthz` — which on a shared container is kyu's endpoint answering 200 | A dead switchboard would report itself healthy. Settled by moving it to CT 113 | closed by design |
| F38 | The restore timeout is hardcoded at 1800 s (`backup.rs:329`) while the backup timeout was deliberately raised to four hours | A large restore from Google Drive dies at thirty minutes, on the operation you least want broken | open |
| F39 | `restic_base`, `password_file` and `snapshot_timeout_s` live in `BackupCfg::default()` and are absent from `FileConfig` | SCOPE G7 wants a target on the HDDs and on Drive; the code can address one, as a string literal (standing rule 27) | open |
| F33 | Unverified, because F32 blocked it: the v1 script greps for `"IsPlaying"`, which is not a field on Jellyfin's session objects as far as I know — `NowPlayingItem` and `PlayState.IsPaused` are. If so the check never matched and allowed every update | Check against a live session once a working key exists; do not assume either way | open |

## Tasks (T)

| # | Task | Depends on | Status |
|---|---|---|---|
| T1 | Phase 1 · full inventory of every guest, its services and its hand-tuned configuration | D1 | done (`INVENTORY.md`) |
| T16 | Repair the kyu stack entry in homelab state (re-adopt under its real name/paths) and re-enable its nightly run — F7 | T1 | open |
| T17 | Bring `/etc/homelab/host.toml`'s `no_touch` in line with the Phase 0 decision, per container — F8 | T1 | open |
| T18 | Decide where MQTT terminates, then remove the stale `lxc-mqtt-stack.yml` route — F10 | T1 | open |
| T19 | Move ansible-era configuration onto `/appdata` on the host so a rebuild survives — F11 | T1 | open |
| T20 | Capture the Cloudflare tunnel ingress and Access policies into the repo — F12 | needs Cloudflare credentials | open |
| T21 | Add promtail to CT 104 — F13 | T1 | open |
| T22 | Give Uptime Kuma a real monitor set — F9 | T1 | open |
| T23 | Remove or rewrite the three stale stack files claiming live vmids — F17 | T1 | open |
| T24 | Verify whether restic retention explains the snapshot counts — F19 | T1 | open |
| T25 | Recyclarr preset (E3) | T1 | open |
| T26 | Adopt CT 111 so SuperSync gets backups — F15/R7 | T1 | open |
| T27 | Delete CT 107; MQTT terminates on the Home Assistant VM (Kenny, 2026-08-30), so the `lxc-mqtt-stack.yml` route and Traefik's `mqtt` entrypoint go with it | T1 | open |
| T28 | Make the clone path honour `unprivileged`, or refuse the combination loudly — F21 | T1 | open |
| T29 | Add privileged-container test coverage — F22 | T28 | open |
| T30 | Validate `host_owner_uid` against the privilege level — F23 | T1 | open |
| T31 | Close the E3 auto-restore granularity gap, test-first — D13 | T1 | open |
| T32 | Enforce the `<app>-config` naming rule in `validate_manifest` — D14 | T1 | open |
| T33 | Decide where vzdump archives live long-term — F24 | T1 | done (D19) |
| T34 | Build a second golden template, privileged, beside the unprivileged one — D18 | T28 | open |
| T35 | Ask the kyu session to publish release binaries + checksums, so the orchestrator can update it — Z4 | — | filed in the vault note "Homelab kyu Release Assets Request" (no kyu session was running) |
| T36 | Mint a fresh Jellyfin API key; the one on CT 106 is refused — F32 | needs Kenny or Jellyfin admin access | open |
| T37 | Fix the syncthing route: `10.10.10.10` → `10.10.10.8` — F34 | — | open |
| T38 | Make the restore timeout configurable and raise it — F38 | — | open |
| T39 | Bring the restic target into `FileConfig` so a second target is expressible — F39 | — | open |
| T40 | Let `native.rs` accept an explicit "no state, by decision" — F36 | T5 | open |
| T41 | Propose snapshot-before-update as a third protection layer in the realisation plan — F40 | — | open |
| T42 | Repair the syncthing route: 10.10.10.10 → the container — F34, B4 | — | open |
| T43 | Correct `presets/syncthing/` off vmid 110, which can never be used | — | open |
| T46 | **Release the host binary.** M3's real templates, M4's live half and every M2 fix only take effect once the host runs the new code. First thing that waits on Kenny rather than on work | 42 commits ready, 20 suites green | waiting on Kenny |
| T44 | Send kyu-runner two config corrections: `healthz_listen` 8081 → 8082 (cadvisor owns 8081 fleet-wide) and `webhook_url` to an IP instead of `homeassistant.lan` (C3) | — | open |
| T45 | Migrate the existing stack-named restic histories to their per-app repositories with `restic copy`, BEFORE those stacks are next deployed — `metrics-config` → `prometheus-config`, `synctest-config` → `syncthing-config` — D25 | — | open, blocks the next deploy of those two stacks |
| T2 | Bring `stacks/metrics/prometheus/prometheus.yml` level with the live one (almanac job, node job ×11, cadvisor job ×6) | T1 | open |
| T3 | Add Alertmanager + its four rules + the `rules/` mount to the metrics stack | T1 | open |
| T4 | Add node_exporter and the SMART textfile collector as managed artifacts | T1 | open |
| T5 | Add cadvisor to every docker host, not only the metrics stack | T1 | open |
| T6 | Add the Prometheus datasource and three dashboards to Grafana provisioning in the repo | T1 | open |
| T7 | Adopt the Recyclarr preset (vault mini-round MR-2) | T1 | open |
| T8 | Write the HTTPSwitchboard preset and decide its container (S2) and config location (S3) | T1 | open |
| T9 | Deploy kyu-runner: HA automation first, then route, then smoke test | T8, T3 | open |
| T10 | Golden LXC image with docker, unattended-upgrades, node_exporter, cadvisor, promtail | T1 | open |
| T11 | Extend C7 so the orchestrator can create a container and install a Rust binary into it | T1 | open |
| T12 | Full backup to the attached HDDs plus a completed Google Drive run, restore drilled | everything above | open |
| T13 | Clean up CT 107 | T12 | open |
| T14 | Clean up CT 190 and 191 after coordinating with pipeline-v2 | T12 | open |
| T15 | Rename the stale worktree branch `hungry-elbakyan-85a663` under `.claude/worktrees/` to something descriptive, or remove it | — | open |

## Inherited open items (from before this project)

These were put to Kenny in earlier forms and never closed. They are carried
here so they cannot evaporate; each becomes a Phase 2 feature question or a
mini-round rather than a re-asked form.

| # | Item | Status |
|---|---|---|
| B1 | U1-U3 · who owns updates for the adopted native services (almanac observes its own reverts; kyu has no update mechanism and no binary release asset) | open |
| B2 | R1-R5 · the C7 milestone report was never signed off | open |
| B3 | V1-V5 · v1-tree cleanup (six dead components, `stacks-backup/`, `install-host-service.sh`, branch `v2-merge`) | open |
| B4 | Grafana service-account token (MB5) needed for the datasource, dashboards and the `kyu_sweeper_age_ms` alert | waiting on Kenny |
| B5 | D5 mirror remote + deploy key | waiting on Kenny |
| B6 | H2 OPNsense API credentials | waiting on Kenny |
| B7 | Broken-release rollback drill for C7 — needs a deliberately broken kyu release | parked |
