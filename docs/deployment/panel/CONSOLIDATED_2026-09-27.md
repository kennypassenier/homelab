# Expert panel 2026-09-27: consolidated findings

Nine read-only reviews of the homelab orchestrator (lenses: system, security, usability, devops, rust, logging, grafana, backup, network) at v3.59.3, merged into one deduplicated list. Findings that describe the same underlying defect are merged and list every reviewer that raised them. Evidence is quoted as the reports give it; "predicted from code" or "not measured" is kept where a report says so. Where reviewers rated the same defect differently, the record says so.

**Totals:** 125 consolidated findings: 4 critical, 34 high, 54 medium, 33 low. 3 are already fixed (fix-41, fix-42). 52 records merge reports from two or more reviewers.

**Kind:** `code` = code fix Claude can do alone; `live` = needs a live change on the named machine (Kenny's go gates outward-facing or destructive changes); `decision` = needs a choice from Kenny first. **Effort:** S (hours), M (a day or two), L (several days).

Severity is the highest rating any reviewer gave unless noted; disagreements are recorded per finding.

## Critical (4)

### Container logs have not reached Loki since 2026-09-03, and the coverage check that should catch it is dead

- **Key:** `container-logs-missing-in-loki` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (all compose containers (Alloy), e.g. CT 106)
- **Reviewers:** grafana, logging
- **Note:** logging rated this HIGH from code alone (no live Loki read); grafana measured the outage in Loki and rated it CRITICAL.
- **Evidence:** Measured (grafana, read-only Loki API): `count_over_time({job="docker"}[1d])` is 0 every day since 2026-09-03 14:39; `label/container_name/values` over 23 days is empty; only `syslog` and `systemd-journal` jobs arrive. Code: `core/src/ops/facts.rs:569-574` only asks Loki when a stack lists an app named `promtail`, which no stack does since the Alloy migration (F256), so `logs_recent` is always `None` and `fleetcheck.rs:1119-1125` can never fire. The deploy's `log shipper` step (`deploy.rs:2289-2345`) only checks `command -v alloy` on ordinary deploys.
- **Failure scenario:** For 24 days nobody could debug a container through Grafana: the home dashboard `homelab-overview`, `homelab-errors`, `docker-containers` and `stack-logs` are blank. A crash-looping app at night leaves no searchable trace, and the nightly check says nothing.
- **Recommendation:** Find the root cause on one container (likeliest: the `alloy` user cannot read `/var/lib/docker/containers`, 0710 root; also check the log driver) and fix the shipper. Re-key the coverage check on what the shipper is (compose stacks: labelled `container_name` lines; native stacks: `unit=` lines), with a failing test first. Check `systemctl is-active alloy` on every deploy. Fix the stale promtail remedy text and the two `checks.yml` comments.

### One offsite copy in one Google account, deletable by the host, never integrity-checked

- **Key:** `single-offsite-copy-no-integrity-check` · **Status:** open · **Effort:** L · **Kind:** needs a decision from Kenny (pve)
- **Reviewers:** backup, security, system
- **Note:** backup rated CRITICAL; security MEDIUM (finding 12, framed as 'no immutable copy'); system folded it into its HIGH H1.
- **Evidence:** Code/config (not measured on Drive): every restic repository lives under `rclone:gdrive:homelab-backups`; `rclone listremotes` returns only `gdrive` (REGISTER F218, open). `grep` finds no `restic check` in `core/` or `host/`. pve root holds both `restic.pw` and the Drive credential and runs `forget --prune` nightly; `wipe` runs `rclone purge` (`core/src/ops/retired.rs:422`). vzdump archives and the ZFS replica sit in the same chassis. The rclone Drive scope is recorded nowhere (gap-31); not measured.
- **Failure scenario:** The Google account is locked, taken over or full; or a host compromise or another F285-class bug prunes the repositories; or a corrupt pack goes unnoticed until the one restore that matters. There is no other copy, and nothing reports the corruption. If the scope is full `drive`, the same credential also reads Kenny's whole personal Drive.
- **Recommendation:** Add a second, independent copy the Proxmox root cannot delete (weekly `restic copy` to an append-only rest-server on another machine, a bucket with object lock, or a rotated USB disk). Add a rotating nightly `restic check` plus a monthly `--read-data-subset`. Measure the rclone scope (`rclone config show gdrive | grep -E '^(scope|root_folder_id)'`) and narrow it to `drive.file` or a dedicated account.

### Tiered retention never kept anything older than about 8 days

- **Key:** `retention-kept-only-eight-days` · **Status:** FIXED (fix-42) · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** Simulated (backup, `retsim.py`, a one-to-one port of `core/src/retention.rs::forget_list`): after 400 nightly runs only ages 0..7 survive. Confirmed live the same day with `restic snapshots`: no repository held anything older than 2026-09-11 (REGISTER fix-42).
- **Failure scenario:** A deleted Paperless document or a silently corrupted *arr database noticed after 9 days would have had no clean copy anywhere; the ZFS replica used the same engine.
- **Recommendation:** Fixed in code by fix-42 (buckets anchored to absolute time, 400-night and 120-night simulation tests). History older than 2026-09-11 is already gone and cannot be recovered. Remaining: ship and verify live after the next nights.

### TUI SHIFT+D outside the repository deployed a synthetic manifest that declares nothing

- **Key:** `tui-deploy-synthetic-spec` · **Status:** FIXED (fix-41) · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** Code-derived (usability): `client/src/tui/model.rs:1618-1638` discarded the `synthetic` flag, so SHIFT+D (and `p` then ENTER) deployed a spec with no files, storage or mounts; since ask-8 a deploy removes what is not declared (`deploy.rs:782-850`, `:2133-2172`).
- **Failure scenario:** Starting the TUI from `~` and pressing SHIFT+D would detach the stack's mounts and delete its compose files, and nightly backups would then read a manifest with no storage.
- **Recommendation:** Fixed by fix-41 part 2: `start_deploy` refuses a synthetic spec (test `fix_41_deploy_refuses_a_stack_without_its_local_stack_file`). Residual ideas from the report, not yet built: a permanent 'view-only: no stack files' banner, and a host-side floor that refuses a deploy removing every mount or file of an existing stack without a flag.

## High (34)

### The orchestrator's own release is unsigned, and the release job gives its build a write token

- **Key:** `host-release-unsigned` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** security, devops, system
- **Note:** Kenny types the minisign passphrase at release time, as for the other projects.
- **Evidence:** `client/src/release.rs:57-62`: `stage_release` uses `signed=false`, checksum only from the same GitHub release. `.github/workflows/release.yml:11-12`: `contents: write` for the job that runs clippy/test/build (every dependency's `build.rs`); `actions/checkout@v7` persists credentials; `softprops/action-gh-release@v3` pinned by mutable tag; no `--locked` on release build (`release.yml:20-25`); no check that the tag equals `Cargo.toml` or is on `main`; `release-update` with no argument installs whatever is `latest` (`client/src/main.rs:925-933`).
- **Failure scenario:** One compromised crate build script, a moved action tag, a stolen gh token or a stray tag push yields a release that the next `homelab release-update` installs as root on the hypervisor, with a checksum that passes by construction.
- **Recommendation:** Sign `SHA256SUMS` with the ecosystem minisign key and flip `stage_release` to `signed=true`; make the host verify `SelfUpdateHost`/`InstallNative` too. Split `release.yml` into a read-only build job (`persist-credentials: false`, `--locked`) and a publish job. Pin actions by SHA; check tag == version and ancestry of `main`.

### The API token is root on pve by several paths; failed attempts are not logged

- **Key:** `api-token-is-root` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** security, network, rust, system
- **Evidence:** `SelfUpdateHost` installs any binary and runs it as root (`core/src/ops/selfupdate.rs`); `install-native --file` checks nothing. `data_mounts.host_path` may be any absolute path (`core/src/manifest.rs:685-697`) and `unprivileged: false` is honoured (`deploy.rs:700-701`). `exec_enabled` gates only `Rpc::ExecIn` (`host/src/main.rs:2865-2867`); `checks.yml` commands run regardless. `wipe`'s typed confirmation is client-side only. `bearer_ok` is a plain `==` (`main.rs:2139-2143`), the 401 branch logs nothing (`:2718-2723`), no rate limit; one static token shared by Garuda, WSL and Windows.
- **Failure scenario:** Malware on any of the three workstation OSes reads the Syncthing-synced token and gets root on the hypervisor; a probe from a compromised container leaves no trace.
- **Recommendation:** Log every 401 with peer address and count it for `doctor`; constant-time compare. Add host-side policy in `host.toml` (`privileged_vmids`, `data_mount_roots`) that deploy refuses to exceed. Verify minisign on self-update (depends on host-release-unsigned). Per-machine tokens with names logged per mutating call.

### pve's management plane (22, 8006, 8443) is reachable on L2 from every container

- **Key:** `pve-management-on-container-vlan` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (pve)
- **Reviewers:** security, network
- **Evidence:** `config/client.toml:15` points clients at `pve:8443` (rescue leg on VLAN 10, gap-13). Daemon listens on `0.0.0.0:8443` (`host/src/main.rs:420-425`). `captured/pve-host/firewall/host.fw` has `enable: 0`. OPNsense's T57 rule does not apply to the on-link pve (F183); only CT 116's `116.fw` drops it. Live firewall state not measured (`nft list ruleset; pve-firewall status`).
- **Failure scenario:** A compromised Jellyfin, FlareSolverr or *arr container talks directly to pve's SSH, Proxmox login and the orchestrator daemon, without passing OPNsense, and can ARP-spoof between the desktop and pve. The T57 goal 'a container must not reach the host' was undone for every guest but one.
- **Recommendation:** Either a pve host firewall with an explicit admin allowlist (22/8006/8443 only from the workstation and the management net, drop the rest of the container subnet), or remove the rescue leg and bind the daemon to pve. Either way add `OUT DROP -dest pve` to every guest.

### Privileged CT 105 (qBittorrent) and CT 106 (Jellyfin, *arr) process hostile internet content

- **Key:** `privileged-cts-hostile-content` · **Status:** open · **Effort:** L · **Kind:** needs a decision from Kenny (pve (CT 105, CT 106))
- **Reviewers:** security, network
- **Evidence:** `stacks/downloader/lxc-compose.yml:58` and `stacks/media/lxc-compose.yml:51`: `unprivileged: false` with nesting; both mount CT 103's datasets read-write. No `idmap` support in `core/src` (`grep -rn idmap` empty). The stated reason is keeping uid 1000 without a `chown -R` over 590 GB.
- **Failure scenario:** A malicious media file or torrent gives code execution in Docker in CT 106, then root in a privileged LXC, then a known escape to root on the hypervisor, with the vault, `restic.pw` and the Drive credential. Even without escape, root in 105/106 can encrypt the 18 TB and 12 TB libraries, which nothing backs up.
- **Recommendation:** Make both unprivileged with a narrow idmap (container 1000 to host 1000, the rest shifted by 100000); only small `/appdata/*-config` dirs need a shift. Add an `idmap:` field to `LxcSpec`. Until then, a host-side privileged-vmid allowlist and ZFS snapshots the containers cannot reach.

### CrowdSec cannot see internet client IPs; every tunnel request comes from a whitelisted internal address

- **Key:** `crowdsec-blind-to-internet` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 104 (gateway))
- **Reviewers:** network, security
- **Note:** network rated HIGH, security MEDIUM (pending the one measurement).
- **Evidence:** Predicted from config, not measured: tunnel ingress `http://the gateway (CT 104):80` from cloudflared on `gateway_net`; Traefik has no `forwardedHeaders.trustedIPs` (`stacks/gateway/traefik/docker-compose.yml:10-41`); bouncer labels set no trusted-IP option (`:64-66`); `stacks/gateway/crowdsec/whitelists.yaml:9-10` whitelists 10/8 and 172.16/12. Measurement: `jq -r .ClientHost /mnt/traefik-logs/access.log | sort | uniq -c | sort -rn | head` on CT 104.
- **Failure scenario:** `sp.kp-soft.dev` (no Access) gets brute-forced and CrowdSec never bans anyone; meanwhile CrowdSec's fail-closed 403 has taken the whole house down twice (F140). Every backend sees one client IP, so per-IP login throttles become one shared bucket and a stranger can lock out a named user.
- **Recommendation:** Measure first. Then trust the `gateway_net` subnet in Traefik and the bouncer, use `CF-Connecting-IP`, and pin the `gateway_net` subnet in the deploy. Run the A8 smoke test through the tunnel. Do this before the apex `kp-soft.dev` goes public. Or drop CrowdSec and stop calling it protection.

### Proxmox and OPNsense admin UIs are on the internet behind one broad, month-long Access policy

- **Key:** `admin-uis-on-internet-broad-access` · **Status:** open · **Effort:** S · **Kind:** needs a decision from Kenny (Cloudflare Access + CT 104)
- **Reviewers:** network, security
- **Note:** network rated HIGH, security MEDIUM.
- **Evidence:** `captured/gateway/cloudflare-access.json` and `CLOUDFLARE.md:48-66`: one wildcard app, 730 h session, three e-mail identities including a third party, Google and one-time PIN, no MFA requirement. Hand-written `manual-routes.yml` on CT 104 routes `opn.kp-soft.dev` and `prox.kp-soft.dev` (`insecureSkipVerify`), confirmed live by F295; `stacks/kp-soft/lxc-compose.yml:137-141`.
- **Failure scenario:** Anyone who reads one of three mailboxes (one outside Kenny's control) requests a PIN and gets a month-long session to the firewall and hypervisor login pages, plus Alertmanager and the Traefik API, which have no login of their own.
- **Recommendation:** Take `prox` and `opn` off the edge (use the LAN or a VPN). Split Access into an admin app (Kenny only, Google only, session of 24 h or less) and a shared app with only what the friend uses.

### Traefik on 0.0.0.0:80 lets any VLAN 10 guest bypass Access and pivot into the management subnet

- **Key:** `traefik-lan-host-header-bypass` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 104 (gateway) + Cloudflare tunnel)
- **Reviewers:** network
- **Evidence:** Traefik publishes `80:80` on all interfaces because the tunnel targets the LXC address (`captured/gateway/cloudflare-tunnel.json`), not `http://traefik:80` as the compose comment claims. Routing is Host-only. Hand-written routes `opn`/`prox` point at the router and pve:8006 (INVENTORY.md:109, F295). Predicted, not tried: `curl -H 'Host: prox.kp-soft.dev' http://the gateway (CT 104)/` from a guest.
- **Failure scenario:** A compromised container on VLAN 10 sends one request with a forged Host header and gets the Proxmox and OPNsense logins from CT 104's position, undoing the OPNsense rule that blocks it, plus the Traefik API map, Alertmanager silences and the *arr apps.
- **Recommendation:** Point the tunnel ingress at `http://traefik:80` and publish Traefik as `127.0.0.1:80:80`; move the Kuma traefik monitor to the ping entrypoint; add an `ipAllowList` on any management router kept. Fix the cloudflared comment and KP_SOFT_MIGRATION step 1.

### Loki's whole API (query, push, delete) is unauthenticated and reachable from every container, including CT 116

- **Key:** `loki-unauthenticated-open` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 104 (gateway))
- **Reviewers:** logging, network
- **Evidence:** `stacks/gateway/loki/loki-config.yaml`: `auth_enabled: false`; `docker-compose.yml`: `3100:3100`; `captured/pve-host/firewall/116.fw` allows CT 116 to the gateway (CT 104):3100. Compactor has `delete_request_store: filesystem` and no `deletion_mode` (effective default not measured). Syslog receiver on `0.0.0.0:1514/udp` accepts any sender.
- **Failure scenario:** The container strangers can reach (CT 116), or any other compromised guest, reads every stack's logs and OPNsense's syslog, pushes forged lines, and asks Loki to delete the evidence of its own intrusion.
- **Recommendation:** Stop publishing 3100 to everyone; expose a push-only path (`POST /loki/api/v1/push`) to the fleet and keep query on `gateway_net` for Grafana. Set `deletion_mode: disabled`. Consider per-stack tenant headers. Allow only the router to 1514.

### The nightly alert is red every night by design: manual checks reopen on every deploy and a deliberate 'nok' cannot be accepted

- **Key:** `nightly-report-always-red` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, usability, devops
- **Note:** Reviewers counted the open items differently (system: seven open; usability: six to confirm, ten answers reopened); both agree on 9 findings total.
- **Evidence:** Live 2026-09-27: `homelab check` shows `9 finding(s)` (usability counts 1 broken, 6 to confirm, 2 noted; system says seven open manual checks) while state shows all 30 manual checks answered. `evaluate_manual` reopens any answer older than the stack's `applied_at` (`core/src/ops/manualchecks.rs:121-147`), and `applied_at` moves on every deploy. kp-soft's deliberate `nok` (D56) is `broken` with no accept state (gap-18). The nightly report goes through `notify_raw` without damper (`host/src/main.rs:2656-2690`), and every success also notifies (`core/src/notify.rs:46-48`). `check` therefore exits 1 every day and cannot gate anything.
- **Failure scenario:** Kenny's phone gets the same red report every night, he learns to ignore it, and a real 'last backup was 60 hours ago' arrives in the same red envelope. From his side, answers he gave that morning do not stick.
- **Recommendation:** Reopen a check only when that app's files or image digest changed (store the hash with the answer); say why it reopened. Add an 'accepted until <date> because <reason>' answer that shows as `noted`. Send the nightly report only when the alarming set changes (or weekly digest); drop success notifications or route them to a log-only topic. Give `check` a severity-based exit code. Allow answering several checks in one go.

### `update_policy: manual` does not stop the nightly round from updating a native service

- **Key:** `native-update-ignores-manual-policy` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Evidence:** `host/src/main.rs:2394-2408` filters `release_update` to Auto, but `:2409-2418` runs `update_native` for every native, which executes the unit's own `update_cmd` (`core/src/ops/native.rs:1205+`, `:1273-1283`). `stacks/kyu/http-switchboard/service.yml:28-39` and `stacks/almanac/service.yml:64,73` have both `update_cmd` and `update_policy: manual`. REGISTER step-16 confirms the kit `update` ran for kyu, kyu-runner and http-switchboard. The `update_cmd` path bypasses `core/src/release_sig.rs`.
- **Failure scenario:** http-switchboard, the one service that translates alarms and was set to manual so a nightly surprise cannot break the alert path, updates itself every night anyway, bypassing the orchestrator's signature check.
- **Recommendation:** Gate `update_native` on the same policy (or add an explicit third value such as `self`), pick one update mechanism per service, and add a scheduler test: a manual native with an `update_cmd` gets no update op.

### A failed update, or one failed backup night, switches off the stack's backups until Kenny intervenes

- **Key:** `failed-update-parks-backups` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops, backup
- **Evidence:** `core/src/ops/backup.rs:67-69`: `parks_the_stack(update_ok) = Failed || !update_ok`; the scheduler sets `enabled=false` (`host/src/main.rs:2443-2472`, `:2507-2531`); `nightly_plan` skips disabled stacks for backup and update (`:2008-2012`). Even a successful rollback returns `Err` (`update.rs:278-283`).
- **Failure scenario:** One bad upstream `bazarr:latest` image, or a Drive 5xx at the backup hour, and the media stack (Jellyfin config included) gets no backups until someone types `homelab enable media`. During a week's holiday that is a week without backups.
- **Recommendation:** Split the flag: park updates per app (with reason and digest), never backups. A failed backup should be retried every night and escalated, not parked.

### The nightly round updates stacks whose backup failed or was deferred that night

- **Key:** `updates-run-after-failed-backup` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system
- **Evidence:** `host/src/main.rs:2477`: `scheduled-update` runs unconditionally after the batch backup; natives at `:2401`, `:2411`. The backup defers for the whole stack when in use, but the update asks only about Jellyfin (`update.rs`).
- **Failure scenario:** Someone watches a film at 02:00, the media backup stands aside, and sonarr, radarr, prowlarr, bazarr and seerr are still updated and migrate their databases with no backup from that night.
- **Recommendation:** Skip every automatic update of a stack whose backup tonight was not `Done`, and log the skip.

### No snapshot before an auto-update: rollback restores the image, not the migrated data (T41)

- **Key:** `no-pre-update-snapshot` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Note:** system folded this into its HIGH H2; devops rated it MEDIUM (M1).
- **Evidence:** Rollback re-tags the old image and force-recreates (`core/src/ops/update.rs:268-290`); it does not undo a schema migration on `/appdata`. REGISTER F40 ('not one exists') and T41 ('snapshot-before-update', handed to this panel) are open. Auto apps with migrations on floating tags: uptime-kuma, jobtracker, seerr, the *arr suite. A rejected digest is pulled again the next night.
- **Failure scenario:** A migrating upgrade fails verification, the old binary runs against the migrated database, and there is no pre-update copy to go back to.
- **Recommendation:** Answer T41 with yes: take a `pct snapshot` (or LVM-thin snapshot of `/appdata`, or a restic/tar of the app's config dir) before each auto-update, restore data and image on rollback, prune on the next success. Record a rejected digest per app and skip it until upstream moves.

### `StateStore::load` treats every read error as 'fresh install' and returns an empty fleet

- **Key:** `state-load-error-empty-fleet` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system
- **Evidence:** `core/src/state.rs:245-248`: `Err(_) => return Ok(HostState::default())`; the doc comment above warns 'the next save would erase every other stack permanently'. `read_file` flattens every error into `CoreError::State(String)`.
- **Failure scenario:** An EACCES, EIO, EMFILE or ENOMEM on the read loads an empty fleet; the next save erases the record of every managed stack, which is exactly what the H7 hardening was built to prevent.
- **Recommendation:** Return the default only for `ErrorKind::NotFound`; keep the error kind through `read_file` and make every other error a hard error.

### Concurrent writers of `state.json` and the notify header file are not serialised; a fixed temp name lets them clobber each other

- **Key:** `state-writes-race` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, rust
- **Note:** system rated HIGH, rust MEDIUM.
- **Evidence:** About a dozen load-modify-save sequences outside `op_lock` (`record_notify_outcome` `host/src/main.rs:2945-2961`, `record_backup_time` `:3176-3186`, `AnswerManualCheck` `:3910-3921`, scheduler writes). The nightly batch runs 3 backups at once (`run_backup_batch`, `buffer_unordered`), each calling `notify` (writes `notify-route-0.header`, `core/src/notify.rs:188-190`) and `record_notify_outcome`. `write_file` uses a fixed `<path>.tmp` (`main.rs:1581-1602`); errors are dropped by `let _ = store.save(...)`; no directory fsync. `StageNativeBinary` writes tens of MB without `op_lock` (`main.rs:3270-3276`). Predicted from code, not observed.
- **Failure scenario:** Two backups finish at the same moment in the night: state.json is torn and quarantined so every mutating operation is refused, or a `last_backup` is silently lost, or the notification header is half-written and the alert is dropped.
- **Recommendation:** One `StateStore::update(|s| ...)` API behind a single async mutex; unique temp names (`<path>.<pid>.<n>.tmp`) and a directory fsync; write the notify header files once at start-up; stage native binaries through `exec.write_file` under the lock.

### The restore drill proves one repository per 90 days (about 7.4 years per cycle), forgets failures, and checks almost nothing

- **Key:** `restore-drill-covers-almost-nothing` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, backup, devops
- **Evidence:** `core/src/ops/restoredrill.rs:23-29` (90-day default), round-robin over 30 live repositories = 2,700 days; the module comment claims 'over a year'. Live: one drill ever (`actual`, 2026-09-03). On failure the index advances and the next pass clears `last_restore_drill_error`. Verdict = 'at least one file, largest not empty' (`:103-119`). host-meta and device repositories are never drilled (op-12, gap-23). Scratch target on pve-root.
- **Failure scenario:** Paperless and its database come up for a drill around 2029. A torn Postgres directory would pass anyway. A failed repository is reported for one night and then not tried again for years.
- **Recommendation:** Drill one repository every night (full rotation in about a month), keep a per-repository last-pass/last-error record, add host-meta and device repositories, check snapshot age, `tar -tf` for native tars, a throwaway Postgres or `PRAGMA integrity_check` for databases, and a scratch target on a data pool with a size check.

### Auto-restore on deploy (E3) treats any restic error as 'fresh' and starts apps with empty data

- **Key:** `auto-restore-error-as-fresh` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** `deploy.rs` auto-restore check (around line 425): any failure of `restic snapshots --last --path` becomes `usable = false` and logs at Info '[e3] ... is empty and has no snapshot, fresh'. Drive unreachable, expired rclone token, wrong password or the 120 s timeout on an empty cache all land here. Predicted from code.
- **Failure scenario:** During a full-host rebuild a slow Drive listing times out; Paperless initialises an empty database, that night's backup makes it `latest`, and the real history is pruned away.
- **Recommendation:** Treat only restic's explicit 'repository does not exist' (exit 10 on restic 0.17+) as fresh; any other error fails the step or keeps the app stopped and its backups blocked until confirmed. Log 'fresh' at Warn and list it in the deploy summary.

### A rebuilt native service starts with empty data and the nightly backup can overwrite its history

- **Key:** `native-rebuild-starts-empty` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** The deploy starts a unit once program, account and env exist; empty `data_dirs` do not block it. `backup_native` has no emptiness guard (a tar of an empty dir is never zero bytes). `homelab restore` refuses native stacks (gap-28); op-11 notes no test exercises the manual procedure.
- **Failure scenario:** After a rebuild, almanac, kyu-runner, http-switchboard or inbox run empty until someone does the manual `restic dump | pct exec tar -x`; if a night passes first, the empty state becomes `latest`.
- **Recommendation:** Before the first start, unpack the newest snapshot into empty `data_dirs` (native equivalent of E3), or refuse to start and to back up a unit whose data dirs are empty while its repository has snapshots. Add a test for op-11.

### The full-host rebuild order deadlocks when the router (OPNsense VM 100) is lost with the host; Home Assistant has no documented way back

- **Key:** `dr-order-deadlocks-without-router` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (pve (VM 100, VM 101))
- **Reviewers:** backup
- **Evidence:** DR 'Full-host rebuild order' step 3 needs internet (rclone for host-meta) while step 5 restores VM 100 from vzdump on `hdd4tb-backup` in the same box. No procedure restores OPNsense `config.xml` from `opnsense-config`. As of 2026-09-02 there was no vzdump job for VM 101 (HA); nothing reads whether one exists now (not measured). Adopted CT 118 has no container file.
- **Failure scenario:** Fire, theft or a surge takes the host and its disks: no router means no internet, no Drive and no host-meta, so the runbook cannot be followed, and Home Assistant, the heart of the house, has no path back.
- **Recommendation:** Add a 'step 0: internet without OPNsense' (ISP modem in router mode or phone tethering). Restore VM 100 first from a local image or fresh OPNsense plus `config.xml` via `restic dump`. Name HA's backup location and restore steps. Add a read-only fleet fact for the newest vzdump per no-touch guest. Keep an offsite copy of the VM 100/101 images or HA's own backup.

### The daemon's systemd unit and OnFailure rollback hook are in no repository and no backup; the DR unit silently drops watchdog and rollback

- **Key:** `daemon-units-outside-repo` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (pve)
- **Reviewers:** devops, system, logging, backup
- **Evidence:** REGISTER gap-31 (open); `docs/DR_RUNBOOK.md:77-93` recreates a minimal unit (`Restart=on-failure`, no `Type=notify`, no `WatchdogSec`, no `OnFailure=`) while the live unit has `Type=notify` + `WatchdogSec` (REALIZATION_PLAN.md:112). `captured/pve-host/` holds only the smart collector. `homelab doctor` does not check the unit. `core/src/ops/selfupdate.rs:39-48` hardcodes `/var/lib/homelab/...` for the marker while the new binary clears `{state_dir}/selfupdate.pending` (`host/src/main.rs:1884`).
- **Failure scenario:** After a host rebuild following the runbook, the daemon runs without watchdog and without self-update rollback, and nothing reports it; the next bad self-update has no safety net.
- **Recommendation:** Commit `homelab-host.service` and the rollback unit/script (e.g. `host/systemd/`), install them from the binary or the self-update op, include them in host-meta, render DR_RUNBOOK from them, and let `doctor` compare `OnFailure`, `WatchdogUSec`, `Type` and the marker path. Derive `SelfUpdateCfg` from `state_dir`.

### Background tasks are not supervised; the watchdog proves only that the runtime is alive; no SIGTERM handling

- **Key:** `background-tasks-unsupervised` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `host/src/main.rs:1853`: `tokio::spawn(scheduler_loop)` with the handle dropped. 19 unwrap/expect in host code, 9 of them `state.settings.read()/write().unwrap()` and 2 `damper.lock().unwrap()` (poisoning). The watchdog is fed by its own 10 s loop (`:1896-1901`); `/api/health` answers a constant 'ok'. No signal handling at all (grep: 0). `READY=1` is sent (`:1895`) before the socket binds (`:1903`).
- **Failure scenario:** One panic in the scheduler ends nightly backups for good without a word, while systemd's watchdog stays happy. A `systemctl stop` or self-update restart kills a step mid-way.
- **Recommendation:** Keep the task handles and `select!` on them so the process exits non-zero when the scheduler dies; feed `WATCHDOG=1` from the scheduler tick; add a SIGTERM handler that stops accepting, waits for `op_lock` with a bound and exits 0 (Kenny's adoption norm N1); replace `.unwrap()` on locks.

### A timeout ends the wait but not the work inside the container

- **Key:** `timeout-leaves-container-work-running` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `RealExecutor::run` (`host/src/main.rs:1545-1569`) uses `kill_on_drop` + `tokio::time::timeout`, killing only `pct`, not `lxc-attach` or the script (no process group). Only `facts.rs:468` uses `timeout` inside the container. The registry-cache fallback (`deploy.rs:1616-1660`) runs `docker system prune -f` and a second pull after a timeout. Reasoned from code, not measured.
- **Failure scenario:** A slow image pull times out; the step is reported failed, but the first pull keeps running next to the prune and the second pull, and changes the container after the operation said it stopped. 'Re-running is always safe' no longer holds.
- **Recommendation:** Spawn with `.process_group(0)` and `killpg(SIGKILL)` on timeout; wrap `pct_sh` scripts as `timeout -k 10 <N> sh -c ...`. Measure once on a scratch CT that a timed-out `sleep 600` leaves nothing behind.

### Host questions during an operation cannot be answered: the CLI says 'use the TUI', and the TUI's answer is stuck behind the running request

- **Key:** `host-questions-unanswerable` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust, usability
- **Evidence:** `host/src/main.rs:2766-2782`: the WebSocket loop handles one RPC at a time inline; the TUI uses one socket for `DeployStack` and `Command::Answer` (`client/src/tui/backend.rs:45-133`, `model.rs:606`, `:1511`), so the answer is not read until the deploy returns. `LiveAsker::ask` waits `ask_timeout_s` (`:1728-1765`). The CLI prints 'run this from the TUI to decide' and times out as `Unattended` after 120 s (`client/src/main.rs:1318-1343`); `apply` stops at the first failed deploy (`:780-789`); the TUI has no apply. Code-derived; no round-trip test exists.
- **Failure scenario:** Kenny removes a route on purpose and runs `homelab apply`; the deploy asks 'routes went from 29 to 28, allow?', the CLI cannot answer, the TUI's answer never arrives, and the deploy fails with an incident bundle.
- **Recommendation:** Spawn a task per request in `ws_session` (mutations are already serialised by `op_lock`), or at least handle `Answer`, `GetState`, `Ping` out of band; add a real WebSocket round-trip test. Let the CLI answer on a TTY, accept a pre-answer such as `--expect-lower routes`, add apply to the TUI, one language per surface.

### The TUI's connection skips the CLI's guards (version gate, repo pin, frame limit), and TOFU still hands the token to whoever answers

- **Key:** `tui-connection-skips-guards` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust, security
- **Note:** rust rated HIGH; security rated the TOFU part MEDIUM (finding 9).
- **Evidence:** Connection setup is written twice: `client/src/main.rs:1220-1380` (CLI) vs `client/src/tui/backend.rs:50-100` (TUI). The TUI lacks the refuse-mutating-to-older-host rule (`main.rs:1357-1375`, built after the 2026-08-31 data_mounts incident), `reconcile_pin` (`:1262`), and the 64 MiB frame limit (fix-30). The TUI TOFUs and saves any certificate (`backend.rs:66-68, 91-95`); the CLI outside the repo also TOFUs (`repo_config.rs:63-73`, `main.rs:1259-1316`), and the bearer is sent right after.
- **Failure scenario:** On a new machine (the planned Windows client), the first TUI or out-of-repo CLI run sends the root-equivalent token to whatever answers at the address; and a TUI deploy to an older host can silently drop fields again, as in the 73-torrent `missingFiles` incident.
- **Recommendation:** One `connect(host, token)` in `homelab_client` that applies pin, frame limit and version gate, used by both CLI and TUI, with a test. Compile the fleet pin into the client (like `RELEASE_PUBKEY`), or never send the bearer on a TOFU connection.

### The transcript secret mask (fix-39 second layer) misses quoted, lower-case, YAML/JSON, URL and Bearer shapes, and never touches the command line

- **Key:** `secret-mask-too-narrow` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging, security, rust
- **Note:** logging rated HIGH (measured); security and rust rated LOW.
- **Evidence:** Measured by logging (compiled `mask_secret_assignments` from `core/src/executor.rs:139-190` verbatim): `KYU_TOKEN="abc"`, `export API_KEY='abc'`, `password=abc`, YAML `KEY: v`, JSON, `postgres://u:p@`, `Authorization: Bearer`, `?api_key=` all pass unmasked; `RESTIC_PASSWORD_FILE=` path is masked. Misses `SUPERSYNC_SMTP_PASS` (security). The `[run ]` line (`executor.rs:215`) is not masked. `~/Projects/dev-procedure/hooks/mask-secrets.sed` is the stricter house standard.
- **Failure scenario:** A quoted token in kyu's systemd EnvironmentFile, or a DSN in a compose env, reaches the client terminal, the pve journal and incident bundles in plain text.
- **Recommendation:** One masker in core ported from `mask-secrets.sed` (case-insensitive, `=`/`:`/JSON shapes, quotes, `scheme://user:pass@`, Bearer, query parameters), applied once at the sink boundary; pin every row of the table with a failing test.

### A failed operation's 'why' carries raw, unmasked, uncapped app output to the journal, Home Assistant and the phone

- **Key:** `error-detail-unmasked-to-phone` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging, rust
- **Evidence:** `core/src/ops/deploy.rs:1919-1934` (`docker compose logs --tail 20` into `CoreError::Command.detail`), `core/src/ops/native.rs:444-451` (`journalctl -n 20`), `executor::run_ok` (full stderr); `core/src/error.rs:69-74` `why = detail`; `host/src/main.rs:3076` (journal), bundle `report.json`, `:2840-2843` notification `error = what :: why` to HA's `/media/homelab_events.log`, logbook and phone. Neither the fix-30 cap nor the fix-39 mask applies.
- **Failure scenario:** A crash-looping app prints its configuration or DSN; that text lands verbatim in HA's log file, the logbook and a phone notification, possibly kilobytes long.
- **Recommendation:** Run diagnostics through the shared masker and `trace_line` before embedding; cap `why` in the notification (about 1 KB, 'full text in <bundle>'); keep the full masked text only in the bundle.

### An unparseable RPC frame is logged whole, and deploy frames carry every `.env` of the stack (latent)

- **Key:** `unparseable-frame-logged-whole` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** `host/src/main.rs:2774`: `error!("unparseable request dropped :: {} :: {}", e, text)`; `DeploySpec.env` is 'the secrets channel' (`core/src/manifest.rs:418-420`); `StageNativeBinary`/`SelfUpdateHost` carry 40 to 95 MB of base64. Latent: not observed.
- **Failure scenario:** An older client against a newer host that made a field mandatory sends a deploy: every secret of that stack, or tens of MB of base64, lands in one pve journal line.
- **Recommendation:** Log the serde error, frame length and method tag only, never the body.

### One full hypervisor disk sends 14 alerts, and the phone keeps only one of them, naming a random host

- **Key:** `disk-alert-fanout-wrong-host` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 113 (metrics) + Home Assistant)
- **Reviewers:** grafana
- **Evidence:** Measured: `count by (host)(node_filesystem_size_bytes{device="/dev/mapper/pve-root"})` = 14 hosts, 27 series (bind mounts). `FilesystemAlmostFull` has no device dedupe; Alertmanager groups by `alertname,host`. `automation.homelab_alert_webhook` uses `ack_id: alert_{{ alert_name }}`, so each delivery replaces the previous one. Same for `HostDown`.
- **Failure scenario:** pve-root reaches 90%: 14 deliveries, and the one notification left on the phone names whichever container arrived last, not the hypervisor.
- **Recommendation:** Count each device once (own rule for hypervisor/pool filesystems, or exclude `/dev/mapper/pve-.*` for `role="lxc"`); key `ack_id` per alert and instance; better, alert on storage via pve-exporter.

### The native 'Service uptime' panel shows the hypervisor's uptime; F248 was closed without being built

- **Key:** `native-uptime-panel-wrong` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** Measured: `time() - node_boot_time_seconds{stack=~"kyu|almanac"}` = 58.5 days on both, although kyu 4.0.1 and almanac 4.0.6 went live today. `core/src/ops/dashboard.rs:62-66` claims it is the service's own uptime. The promised service counters are not generated; tests (`core/tests/monitors_tests.rs:137-181`) only check strings.
- **Failure scenario:** Kenny asks 'is kyu alive?' and the dashboard answers with a confident, wrong number that never resets, even when the unit keeps dying.
- **Recommendation:** Show `node_systemd_unit_state{state="active"}` per unit, enable systemd start-time metrics and plot time since start, add the already-scraped kyu/almanac counters, reopen F248.

### The nightly backup stops Prometheus, Alertmanager and Loki: false 'down' alerts every morning, a log gap, and their data kept on Drive indefinitely

- **Key:** `backup-pause-stops-monitoring` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (CT 113 (metrics), CT 104 (gateway), CT 107 (Kuma))
- **Reviewers:** grafana, logging
- **Evidence:** Measured (grafana): Prometheus and Alertmanager started 2026-09-27 02:11:11 UTC; `todo.notifications` holds 'prometheus is down' and 'alertmanager is down' (ECONNREFUSED) on the first night after Kuma started notifying HA. Loki carries `com.homelab.backup.pause=true` and is stopped for the whole gateway snapshot loop (`backup.rs:596-631`); Alloy's `loki.write` has no WAL and drops a batch after about 8-9 min; UDP syslog is lost meanwhile (code, not measured). `loki-config` and the TSDB fall under the default tiers including 'every 2 months forever' (`core/src/retention.rs`).
- **Failure scenario:** Every morning the notification list shows cry-wolf 'down' items for the monitoring stack; no alert rule is evaluated during the pause; the log gap falls exactly in the window when the nightly round runs; and anything an app should not have logged lives on Drive for good.
- **Recommendation:** Stop pausing Prometheus (TSDB snapshot via admin API, internal only, or exclude the TSDB from backup); back up only Loki's configuration or give it a short tier; declare a Kuma maintenance window for the backup round; add a WAL or larger retry budget to `loki.write`; measure the gap once around the backup hour.

### The alert path to the phone drops, duplicates and speaks alerts

- **Key:** `phone-path-loses-information` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 109 (http-switchboard) + Home Assistant + CT 107 (Kuma))
- **Reviewers:** grafana
- **Evidence:** `captured/messaging/http-switchboard/config.example.toml` sends `alerts.0.status`; the HA automation keeps only `firing`, so a mixed group whose first member resolved is dropped whole. Both automations push only on down/firing; nothing clears a notification on recovery. A down media CT gives 1 Kuma ping + 6 Kuma HTTP + 1 `HostDown` = 8 pushes. Kuma priority comes from one global helper; its action sets `notification_types: ["push","tts"]`. Read from HA configs (read-only).
- **Failure scenario:** A disk alert where one mount resolved and another still fires never reaches the phone; a single Bazarr flap is spoken aloud in the house; after a real outage Kenny cannot tell 'still broken' from 'fixed at 03:00'.
- **Recommendation:** Use Alertmanager's top-level `status` and send firing and resolved lists separately; key `ack_id` per instance or monitor and update the notification to 'back up' on recovery; per-monitor priority in the seeder; suppress per-app monitors while the host ping is down; no TTS below critical.

### doctor, check, status and the TUI give four different answers to 'is anything waiting for me?'

- **Key:** `four-answers-to-is-anything-wrong` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability, system
- **Evidence:** Live, same minute: `doctor` green Ok (24.3 s), `check` red 9 findings exit 1 (41.1 s), `status` 1,473 lines of raw `state.json` with epoch times (`host/src/main.rs:3226-3240`), TUI ticker 'ALL SYSTEMS NOMINAL' that never reads check findings or incidents (`client/src/tui/view/mod.rs:596-660`). The morning check (op-2) takes four verbs and over 65 s.
- **Failure scenario:** Kenny opens the TUI, sees 'ALL SYSTEMS NOMINAL', and misses the one broken item that `check` lists seventh of nine.
- **Recommendation:** One verb (`homelab today` or bare `homelab`) and one TUI panel merging doctor, check, open incidents and manual checks, sorted by severity, one line and one remedy each, ending in 'Nothing needs you' or 'N things need you'. Turn `status` into a human table and put raw JSON behind `--json`. The ticker uses the same verdict or goes.

### Restore on the CLI has no confirmation, takes no pre-restore snapshot, and offers no way to pick a snapshot

- **Key:** `restore-no-confirm-no-safety-snapshot` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `client/src/main.rs:870-893`: `homelab restore` goes straight to `RestoreStack`; the TUI asks for a typed name (`model.rs:801-825`). Host restore steps take no snapshot of current data (`core/src/ops/backup.rs:870-1010`). Picking an older snapshot needs a root shell and three `export RESTIC_*` lines (op-10). Code-derived.
- **Failure scenario:** Corruption happened before last night's backup, so `latest` is the corruption; a CLI restore overwrites the current state without asking, and that state is gone too.
- **Recommendation:** Typed name on the CLI (`--yes` for scripts); automatic pre-restore snapshot with its id printed; `homelab snapshots stacks/<name>`; let the TUI show that list before restoring.

### A TUI refresh runs latch and downloads release binaries on the UI thread, and prints into the screen

- **Key:** `tui-refresh-blocks-on-downloads` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** Code-derived: on every `ServerMsg::State` the TUI calls `build_spec` per local stack for the drift badge (`client/src/tui/model.rs:490-509`); `build_spec` runs `latch cat` and `gh release download` per native with no cache (`client/src/spec.rs:112-119,187-235`, `client/src/release.rs:82-108`), kyu alone about 71 MiB (F303), synchronously, and writes with `eprintln!`/`println!` onto the raw-mode screen.
- **Failure scenario:** Pressing `r` freezes the TUI while it downloads tens of MB, then leaves stray text over the display.
- **Recommendation:** Compute the intent hash from manifest and files without secrets or binaries; never shell out from `update()`; route output through the model's log.

## Medium (54)

### `homelab apply` deploys without asking and has no dry run; it would create the throwaway drill container CT 119

- **Key:** `apply-no-confirm-creates-drill` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, usability
- **Evidence:** `client/src/main.rs:780-790` deploys each plan entry straight away; no `--dry-run` exists. `scan_local_stacks` takes every dir with an `lxc-compose.yml` (`client/src/spec.rs:415-427`), including `stacks/drill` (vmid 119, 'destroy it in the same sitting'), not in host state; DR runbook lists `119-app-drill` (`docs/DR_RUNBOOK.md:271`). `homelab plan` prints counts, not a per-file diff (`main.rs:671-690`). Since ask-8 deploys also remove things.
- **Failure scenario:** The next `homelab apply` creates CT 119, which is then backed up nightly and appears in monitors, the dashboard and the homepage.
- **Recommendation:** An `ephemeral: true` (or `apply: false`) key that apply, runbook and check skip, or move drill out of `stacks/`. Show the plan and ask once (`--yes`, `--dry-run`); give `plan` the per-file diff including removals.

### `homelab check` never compares the repository with what the host applied

- **Key:** `check-blind-to-repo-drift` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Evidence:** The client sends only `(dir, vmid)` pairs (`client/src/main.rs:310-337`); `fleetcheck.rs:660-715` never compares the intent hash. Drift is computed only in the TUI (`model.rs:495-506`) and inside `apply`, which then acts. The nightly check has no stack files (`host/src/main.rs:2629-2635`). Nothing diffs `/opt/<stack>` in containers with the host's intent repo. No OS patch-state check (`apt list --upgradable`, `reboot-required`).
- **Failure scenario:** An edited but undeployed stack, a never-deployed stack, or an out-of-band edit inside a container stays invisible until the next deploy overwrites or acts on it.
- **Recommendation:** Send the local intent hash and emit a `drift` finding; add `apply --plan` with exit codes (0 in sync, 2 pending); nightly hash-diff of `/opt/<stack>` against the intent repo; a patch-state finding.

### Manifests clone Debian 12 templates while some containers were upgraded to Debian 13 in place

- **Key:** `os-drift-debian-templates` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (all containers)
- **Reviewers:** system, devops
- **Evidence:** Every manifest says `clone:997`/`clone:998` (Debian 12); CT 109 runs Debian 13 after an in-place upgrade (REGISTER step-2); F307 names CT 112 and CT 118 too; Debian 13 templates 995/996 exist unused. D113 ('the whole fleet goes to Debian 13', 2026-09-09) has not moved since CT 109; 11 of 14 containers on bookworm. Nothing compares OS with the manifest.
- **Failure scenario:** A C4 replacement or DR rebuild of kyu from the files puts it back on Debian 12, where the glibc of its current binary may not be available.
- **Recommendation:** Declare the OS (`os: debian-13`), resolve the template from it, and let the check compare `/etc/os-release`. Either finish D113 or record 'bookworm until <date>'.

### inbox (CT 118) has no container file: cores, RAM, disk, network and protection are unmanaged and nothing rebuilds it

- **Key:** `inbox-ct118-unmanaged` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, backup
- **Evidence:** Only `service.yml`; state record has `manifest: null`; `apply` skips it; DR text says 're-register with `homelab adopt`', which assumes a hand-built container.
- **Failure scenario:** After a host loss nothing recreates CT 118, so the inbox service has nowhere to run until someone builds a container by hand.
- **Recommendation:** Give inbox an `lxc-compose.yml` with `natives: [inbox]`, as kyu and almanac have.

### 'Manual' images run `:latest` with no pinned digest and no 'update available' signal; a rebuild pulls whatever is latest that day

- **Key:** `manual-images-latest-unpinned` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny
- **Reviewers:** system, devops, grafana, security
- **Note:** system and devops rated MEDIUM, grafana LOW.
- **Evidence:** Grep over `stacks/*/*/docker-compose.yml`: about 18-20 images `:latest` or untagged, 3 digest-pinned of 34 (security); traefik, crowdsec, cloudflared, grafana, prometheus, alertmanager, pve-exporter, goaccess, qbittorrent, cadvisor are `:latest` with policy `manual`. No code records running digests or reports a newer upstream (grep empty). Templates bake Docker via `curl -fsSL https://get.docker.com | sh` and `cadvisor:latest` (`core/src/ops/template.rs:194`, `:233`).
- **Failure scenario:** CrowdSec, Traefik and cloudflared at the internet edge stay on their install-day version until someone remembers; a disaster rebuild pulls a new major (Traefik v3 to v4, Grafana 13) on the gateway at the worst moment, unrecorded.
- **Recommendation:** Pin by digest in compose files and let `homelab update` rewrite the pin as a commit; Dependabot/Renovate for docker-compose; record running `RepoDigest` in state; a nightly `noted` finding when a manual app is N days behind upstream; pin Docker and cadvisor in templates.

### Images are pulled over plain HTTP from the LAN registry cache, including into the privileged containers

- **Key:** `registry-cache-plaintext` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 117 (registry) + CT 105/106)
- **Reviewers:** security
- **Evidence:** Deploys rewrite images to the pull-through cache on the registry (CT 117) (`core/src/ops/registry_cache.rs:1-20`); docker daemons get `insecure-registries` (`core/src/ops/guards.rs:209`); most images not digest-pinned.
- **Failure scenario:** A container that ARP-spoofs the registry (CT 117), or a compromise of CT 117, serves a modified image the next time CT 105 or CT 106 pulls a tag.
- **Recommendation:** TLS on the cache with a small house CA distributed by the guards step, or run the privileged stacks without the cache; pin the images of 105 and 106 by digest.

### A healthy native update deletes every local rollback copy after a 10-second health window

- **Key:** `native-rollback-copies-deleted` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system
- **Evidence:** `health_script` accepts a unit active and not restarting for 5 x 2 s (`core/src/ops/native.rs:732-747`); `update_native` then removes `.homelab-prev` and the kit's `.prev` (`:1314-1345`, `:811-813`). fix-10 did this for a 2 GB disk; CT 109 now has 4 GB.
- **Failure scenario:** A kyu release that starts fine and misbehaves an hour later has no N-1 binary on disk, on the notification path.
- **Recommendation:** Keep exactly one previous binary after a healthy update; add `homelab rollback-native <stack>/<unit>`.

### Ingress and the fleet's observability share one container with no memory limits and `protection: false`

- **Key:** `gateway-shared-no-limits` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (CT 104 (gateway), CT 113)
- **Reviewers:** system
- **Evidence:** `stacks/gateway/lxc-compose.yml`: traefik, cloudflared, crowdsec, grafana, loki, goaccess in 5,120 MiB, `protection: false` (line 49); no `mem_limit` in any gateway compose file. `protection: false` also on uptime and syncthing without comment (L8).
- **Failure scenario:** A Loki query storm competes with Traefik in one cgroup; when the gateway goes down, the logs and dashboards that would explain the outage go down with it.
- **Recommendation:** Move Loki and Grafana to CT 113 (metrics), or give each gateway service a `mem_limit` with Traefik protected; set `protection: true` or document why not.

### The nightly round and RPC dispatch are untested glue in a 4,447-line `host/src/main.rs`; `deploy()` is one 2,233-line function

- **Key:** `host-monolith-untested` · **Status:** open · **Effort:** L · **Kind:** code fix Claude can do alone
- **Reviewers:** system, rust
- **Evidence:** `host/src/main.rs` 4,447 lines, 18 tests, none for `scheduler_loop` (361 lines) or `run_backup_batch`; `handle_rpc` 694 lines. Host takes `&RealExecutor` concretely (22 refs), so it cannot run against `MockExecutor`; 25 direct `std::fs` calls. `deploy()` 2,233 lines (`deploy.rs:224`), shared state via `Arc<Mutex>` in sequential code. Client `main()` 975 lines of hand-rolled parsing (root of F309). Measured with clippy `too_many_lines` and wc.
- **Failure scenario:** Every decision the backup/update faults above depend on lives in this untested loop, so a fix there cannot be proven by a test today.
- **Recommendation:** Move the night into core as a pure `night(plan, outcomes) -> Vec<StateChange|Notify>` tested with `MockExecutor`; make host take `&dyn Executor`; split main.rs (config, rpc, scheduler, notify); split deploy steps into functions; `clap` for the client; `too_many_lines` ratchet at 300.

### Resume and operator documents contradict each other and the code

- **Key:** `docs-contradict-each-other` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, usability, logging, network, backup, devops
- **Evidence:** REGISTER.md is 543 KB with a 4,061-char line (Read refuses it); CLAUDE.md says host v3.57.0 and v3.59.3 in two places; README says 3.58.10 (`README.md:17`) vs `Cargo.toml` 3.59.3; test counts 552/562/211. `unfinished-features.md` points to a missing dir. USER_GUIDE (2,358 lines) is ordered by feature ID with 438 file:line citations. BACKUP_MODEL.md is stale (one repo per stack). DEVELOPMENT.md says CI blocks merges; branch protection has `enforce_admins=false` and no PR requirement. Network/logging doc drift: cloudflared comment and KP_SOFT_MIGRATION ingress target, `platform_net`, promtail references, NOTIFICATIONS_INVENTORY Loki alertmanager_url.
- **Failure scenario:** The next session resumes from these files and acts on a wrong version or state claim, as happened before (F163); in a disaster Kenny opens BACKUP_MODEL.md instead of the generated runbook.
- **Recommendation:** Generate every number and version (`make docs-facts`) and fail the gate when stale; a one-screen RESUME; archive closed REGISTER rows per phase; a one-page task card for Kenny; take file:line out of operator docs; mark BACKUP_MODEL.md superseded; fix the listed drifts.

### The compose update policy is read from the first container only and applied to the whole app

- **Key:** `compose-policy-first-container` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Note:** system bundled this in its HIGH H5; devops rated it LOW.
- **Evidence:** `app_policy` runs `docker compose ps -q | head -1` (`core/src/ops/update.rs:63`). `stacks/paperwork/paperless-db` labels postgres `manual` and redis `auto`; `stacks/gateway/goaccess` labels goaccess `manual` and goaccess-report/nginx `auto`.
- **Failure scenario:** Depending on container order, Postgres gets a nightly recreate its label forbids, or the `auto` service never updates.
- **Recommendation:** Evaluate the label per service and update per service (`pull <svc>`, `up -d <svc>`), or refuse mixed labels within one app at validation.

### Compose auto-update verification asks 'did one service start', not 'does every service stay up and healthy'

- **Key:** `compose-update-verify-weak` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Note:** system bundled this in its HIGH H5; devops rated it MEDIUM.
- **Evidence:** `verify_app` = `docker compose ps --status running --services` non-empty, read once right after `up -d` (`core/src/ops/update.rs:70-83`, `:246-262`); Docker healthcheck and `checks.yml` probes not consulted. F300 fixed exactly this for natives (settle window + unchanged `NRestarts`) but was never ported.
- **Failure scenario:** A two-service app with a crashed Postgres, or a container in a restart loop that is 'running' part of each cycle, passes the nightly update as good.
- **Recommendation:** Port F300: settle window (about 60 s), every service running, `RestartCount` unchanged, `healthy` where a healthcheck exists, then the app's `checks.yml` probes.

### pve-exporter on LAN :9221 forwards its PVE API token to any `?target=` without TLS verification

- **Key:** `pve-exporter-forwards-token` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 113 (metrics) + pve)
- **Reviewers:** security, network
- **Evidence:** `stacks/metrics/pve-exporter/docker-compose.yml:12,14`: `PVE_VERIFY_SSL=false`, publishes `9221:9221`; `/pve?target=` is caller-controlled (`checks.yml:7`). Token role only described as 'PVEAuditor suffices', not verified (`pveum user token permissions`).
- **Failure scenario:** Anyone on the LAN calls `http://metrics (CT 113):9221/pve?target=<attacker>` and receives the exporter's PVE token, giving at least a full read of the hypervisor configuration.
- **Recommendation:** Do not publish 9221 (Prometheus reaches it on `metrics_net`); verify the token is a privilege-separated PVEAuditor token; give the exporter the PVE CA instead of `verify_ssl=false`.

### The workstation root SSH key to pve and every container has no passphrase and is synced across three OSes

- **Key:** `ssh-key-no-passphrase` · **Status:** open · **Effort:** S · **Kind:** needs a decision from Kenny (workstations + pve)
- **Reviewers:** security
- **Evidence:** Measured locally: `ssh-keygen -y -P ""` succeeds on `~/.secrets/ssh/id_ed25519`; `~/.ssh/config` uses it for `User root` on pve and every CT; `~/.secrets` is a Syncthing folder shared by Garuda, WSL and Windows.
- **Failure scenario:** User-level malware on any of the three OSes gets root on the hypervisor, bypassing pin, token and every guard.
- **Recommendation:** Add a passphrase and use ssh-agent, or an `ed25519-sk` FIDO2 key for pve; add `from="the workstation"` in pve's `authorized_keys`.

### The public repository publishes the Access allowlist (including a third party's email) and the attack map

- **Key:** `public-repo-attack-map` · **Status:** open · **Effort:** S · **Kind:** needs a decision from Kenny
- **Reviewers:** security, devops
- **Evidence:** `.githooks/check-secrets.sh:4` confirms the repo is public; `docs/deployment/CLOUDFLARE.md:56-58` lists three Access-allowed addresses including a friend's personal email, plus account, zone, tunnel and service-token ids. Other docs publish privileged CTs, internal IPs and CrowdSec's state. The secret gate matches only HA webhook ids (`check-secrets.sh:22-24`); no server-side secret scan (devops L4). Security scanned the tree and found no live secret.
- **Failure scenario:** Targeted phishing of the three OTP mailboxes is easy with the list public; a friend's address sits in public git history.
- **Recommendation:** Redact the email list and service-token id (history keeps them, so tell the person); decide whether `docs/deployment/` belongs in a public repo; replace the single-shape gate with `gitleaks` locally and in CI.

### Self-update accepts a new daemon after 5 s alive, before its listener binds, and the client never learns whether it was rolled back

- **Key:** `self-update-acceptance-weak` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** devops, rust
- **Evidence:** Acceptance is 'alive for 5 s' (`host/src/main.rs:1882-1891`); `READY=1` is sent at `:1895` before the TLS listener binds at `:1903`. `release-update` returns as soon as the restart is scheduled (`client/src/main.rs:947`); a rollback is only visible via a notification sent by the unversioned script.
- **Failure scenario:** A new binary that runs but cannot talk to the client (protocol change, broken RPC) is accepted, and the client can no longer ship the next fix; only ssh plus the manual DR steps remain.
- **Recommendation:** Accept only after an authenticated RPC round-trip (or at least WS answered and scheduler alive); send `READY=1` after bind; make `release-update` wait up to about 60 s for the new version and exit non-zero on rollback or timeout.

### The orchestrator's own traces live only on pve: not shipped, not backed up, never pruned

- **Key:** `orchestrator-logs-only-on-pve` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (pve)
- **Reviewers:** logging, system, rust
- **Evidence:** REGISTER fix-39 measured alloy and promtail inactive on pve; no journald cap in the repo; `backup_host_meta` (`core/src/ops/backup.rs:1058-1100`) excludes `incidents/`, `journal.jsonl`, `audit.log`. 'Nothing prunes bundles' (DEBUGGING_GUIDE §3.4, 90 bundles today). `journal.jsonl` read whole at every boot and failure (`host/src/main.rs:1814`, `:4418`; `core/src/incidents.rs:89-103`) with blocking `std::fs` on the runtime. `homelab incidents` lists names only.
- **Failure scenario:** At night the only remote trace is one notification line; everything else needs a root shell on the hypervisor, and on pve disk loss the exec audit trail and every bundle go with it.
- **Recommendation:** After fixing Loki's exposure and the mask: ship pve's journal (homelab-host, kernel, zfs-zed, smartd, pve*) via Alloy with redaction; add `homelab incidents show <name>`; prune bundles by age/count; bound `journal.jsonl`; add `incidents/` and `audit.log` to host-meta; capture a journald drop-in and let doctor report sizes.

### Journal lines carry no operation, stack or step (AR15 not built) and include ANSI escape codes

- **Key:** `journal-lines-lack-op-context` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging, system
- **Evidence:** `docs/ARCHITECTURE_DECISIONS.md:26` (AR15) vs `ARCHITECTURE_REFERENCE.md:480` 'stderr only; no ring'; `BroadcastSink::emit` (`host/src/main.rs:1627-1631`) logs only `source=HOST` and the message; three backups interleave (`:2231`). tracing-subscriber ANSI on unless `NO_COLOR`; no `.with_ansi(false)`. The ANSI rendering in journalctl is 'verify on pve' (not measured).
- **Failure scenario:** At 02:00 a line like `image not found` cannot be attributed to one of three concurrent backups, and `grep 'WARN scheduler'` fails on escape codes.
- **Recommendation:** Wrap each `run_op_locked` in an `info_span!` with op and stack; log step start/finish to the journal; `.with_ansi(false)` or JSON for journald; implement the JSONL ring or amend AR15.

### Alloy is never auto-updated (the migration's stated reason is false) and Loki is pinned for a reason that no longer exists

- **Key:** `alloy-not-updated-loki-stale-pin` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (all containers (Alloy) + CT 104 (Loki))
- **Reviewers:** logging
- **Evidence:** `logshipper.rs:13-18` says unattended-upgrades keeps the shipper patched; `guards.rs` `UNATTENDED_UPGRADES` allows only Debian-Security, which Grafana's apt repo does not match; `homelab patch` is never scheduled. `stacks/gateway/loki/docker-compose.yml:20-27` pins `grafana/loki:3.0.0` 'because promtail ... is pinned to the same version'; promtail is gone.
- **Failure scenario:** Alloy on 13 containers stays at its install-day version and Loki on a 2024 release indefinitely; the EOL situation F249 migrated away from returns.
- **Recommendation:** Add Grafana's origin to the unattended-upgrades pattern (read `apt-cache policy alloy`) or schedule `patch`; upgrade Loki deliberately and replace the stale pin comment.

### Docker log lines carry the time Alloy read them, not the time they were written

- **Key:** `docker-log-timestamps-wrong` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** The `loki.process "docker"` stage extracts `log`, `stream`, `attrs` but not `time`, and has no `stage.timestamp`; syslog file and receiver also use receive time.
- **Failure scenario:** After an Alloy restart or backlog, a burst of old lines is stamped 'now', and 'what happened at 02:07' misleads.
- **Recommendation:** Extract `time` in the first `stage.json` and add `stage.timestamp { source = "time" format = "RFC3339Nano" }`, with a test on the rendered block.

### Error dashboards query only `job="docker"`, and generated per-stack panels have stray series, no units or legends, and wrong grouping

- **Key:** `dashboards-blind-to-native-and-sloppy` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** logging, grafana
- **Evidence:** Every error panel in `homelab-errors.json`, `docker-containers.json`, `homelab-overview.json` queries `{job="docker"}`; kyu, kyu-runner, http-switchboard, almanac ship as `systemd-journal`, OPNsense as `syslog`. Generator (`core/src/ops/dashboard.rs`), checked against live series: CPU panel without `name!=""` (largest line `id="/"`), no `legendFormat`, no units, native stacks grouped by `container_name`, OPNsense syslog counted as `stack="gateway"` (61 of 61 errors), loose error regex with default threshold.
- **Failure scenario:** The notification hub every alert travels through is missing from 'Homelab Errors'; the gateway's error count is mostly firewall syslog.
- **Recommendation:** Query `{job=~"docker|systemd-journal"}` and relabel journal priority to `level`; add a firewall panel; reuse the committed `homelab-containers.json` definitions in the generator; group native panels by `unit`; give OPNsense its own stack label.

### The Grafana home dashboard answers 'what is being logged', not 'is anything wrong'

- **Key:** `home-dashboard-wrong-question` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** `homelab-overview` has three panels (log tail, log rate, 24 h errors); none shows services down, disks filling, stale backups or firing alerts.
- **Failure scenario:** Opening Grafana during an incident shows log volume, not which service is down or which disk is filling.
- **Recommendation:** Make the home page a status board: firing `ALERTS`, targets down, fullest filesystems and storages, SMART and zpool state, failed units, kyu pending, time since last backup; link to stack dashboards.

### The debugging guide never mentions Loki, Alloy or Grafana

- **Key:** `debugging-guide-no-loki` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** DEBUGGING_GUIDE.md (1,072 lines) has zero mentions outside one step name; §1's trace table has no row for container logs.
- **Failure scenario:** At night the question 'which container misbehaved and what did it say' has no documented route.
- **Recommendation:** Add a section with the label schema, five canned LogQL queries and the Alloy health reading.

### The Prometheus TSDB lives on the hypervisor's root filesystem with no size cap

- **Key:** `tsdb-on-pve-root-no-cap` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 113 (metrics))
- **Reviewers:** grafana
- **Evidence:** Measured: `/appdata/metrics/prometheus-config` on `/dev/mapper/pve-root` (100.9 GB, 51% used); TSDB 6.33 GB growing +0.22 GB/day toward about 20 GB at 90 d; only `retention.time=90d`, no `retention.size`.
- **Failure scenario:** An exporter upgrade or a label-per-request metric blows up cardinality and fills the Proxmox root filesystem (pmxcfs, /var/lib/vz, logs); the alert arrives 14 times.
- **Recommendation:** `--storage.tsdb.retention.size=15GB`; a `predict_linear` rule on pve's root; consider `sample_limit` on file_sd jobs.

### Nothing watches the alert chain itself; the only proof is a manual quarterly test

- **Key:** `alert-chain-unwatched` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 113 (metrics) + CT 107 (Kuma))
- **Reviewers:** grafana, system
- **Evidence:** `prometheus.yml` does not scrape Alertmanager, Grafana, Loki, Traefik, kyu-runner or http-switchboard; no Watchdog rule; Kuma checks only `/-/healthy`. F222: kyu answers 401 on a rotated token. System notes 'delivered' means kyu answered 2xx and the host-to-HA chain is 'not proven end to end' (F85).
- **Failure scenario:** A rotated publish token makes every alert fail at Alertmanager; the only trace is Alertmanager's own log, which currently does not reach Loki.
- **Recommendation:** An always-firing Watchdog rule routed to a Kuma push monitor (repeat about 5 min); scrape Alertmanager and alert on `alertmanager_notifications_failed_total`.

### Alert rules are blind to missing data, and strong signals already scraped have no rule; 12 units are failed now

- **Key:** `alert-rules-blind-to-missing-data` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 113 (metrics) + fleet (failed units))
- **Reviewers:** grafana
- **Evidence:** Measured: SMART rules fire only while the metric exists (a drive that stops answering disappears; no freshness rule); `node_zfs_zpool_state` has no rule (four single-disk pools); 12 systemd units failed now (docker-prune on kyu/almanac, ssh on metrics/syncthing, openipmi on 4, nvmf-autoconnect on 2); `HostDown` covers only `job="node"`; almanac R12 counters have no rule.
- **Failure scenario:** A dying drive that stops answering SMART simply disappears from the metrics and nothing fires; a pool going degraded is never alerted.
- **Recommendation:** About six rules: SmartCollectorStale, DriveMissing, ZpoolNotOnline, TargetDown for every job, AlmanacJournalUnreadable, SystemdUnitFailed on an allowlist. Fix or mask the 12 template leftovers first.

### pve-exporter data is collected and used nowhere; the thin pool is invisible

- **Key:** `pve-exporter-data-unused` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 113 (metrics) + CT 104 (Grafana))
- **Reviewers:** grafana
- **Evidence:** Measured: `pve_disk_usage_bytes/pve_disk_size_bytes` gives HDD12TB 78%, local 45%, HDD18TB 33%, local-lvm 23%; no dashboard or rule queries `pve_*`; `local-lvm` (every CT rootfs) is not visible to `node_filesystem`.
- **Failure scenario:** Thin-pool overcommit, the classic way a Proxmox box dies, happens without any panel or alert.
- **Recommendation:** One 'Storage (Proxmox view)' row and a `PveStorageAlmostFull` rule at 85% per storage id; this replaces the fan-out-prone pool part of the disk rule.

### kyu's metrics show a stuck 28k-message backlog and test debris, and nothing looks at them

- **Key:** `kyu-backlog-invisible` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 109 (kyu) + CT 113)
- **Reviewers:** grafana, system
- **Evidence:** Measured: `kyu_deliveries{topic="notify.kenny",subscription="ha",state="pending"}` = 28,342 (28,263 a week ago); four `authtest-*` subscriptions hold 80 each; `kyu_messages` = 28,751. No dashboard or rule reads `kyu_deliveries`. CLAUDE.md mentions 28,335 held messages.
- **Failure scenario:** The transport every notification depends on has a consumer nobody drains; a real backlog on the alert subscriptions would be just as invisible.
- **Recommendation:** A committed kyu dashboard; a pending-growth rule for `switchboard` and `ha-runner`; hand the `notify.kenny/ha` backlog and the authtest leftovers to the kyu project.

### Traefik exposes no metrics: no 5xx or latency per router at the edge

- **Key:** `traefik-no-metrics` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 104 (gateway) + CT 113)
- **Reviewers:** grafana
- **Evidence:** The Traefik command line has no `--metrics.prometheus`, and there is no scrape job.
- **Failure scenario:** One route returning 502, or CrowdSec returning 403 to some clients, goes unseen between Kuma's one GET per minute.
- **Recommendation:** Enable Traefik Prometheus metrics on an internal entrypoint, add a scrape job and a '5xx rate per service' panel, optionally a sustained-5xx alert.

### host-meta is not watched, not drilled, never pruned, and misses files a rebuild needs

- **Key:** `host-meta-gaps` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** op-12 'Nobody watches it'; no doctor or fleet line; no retention step (keeps every rotated secret forever). Missing: `/etc/network/interfaces`, `/etc/pve/storage.cfg`, `jobs.cfg`, `qemu-server/100.conf` and `101.conf` (Zigbee USB passthrough), `99-homelab-swappiness.conf`, rclone remote settings (units covered in daemon-units-outside-repo). Layer 3 restores `latest` without a host or id filter.
- **Failure scenario:** After a host loss the VLAN bridges, storage definitions and HA's USB passthrough must be rebuilt from memory; if a fresh daemon snapshots host-meta first, `latest` is the empty host.
- **Recommendation:** A fleet finding when `last_host_meta` is over 48 h; add the tiny files; long-horizon retention; Layer 3 restores by snapshot id and says the daemon must not start before it.

### The restic password and credential chain has a bus factor of one and is checked only when someone remembers

- **Key:** `password-chain-bus-factor` · **Status:** open · **Effort:** S · **Kind:** needs a decision from Kenny
- **Reviewers:** backup
- **Evidence:** One password opens every repository; its only independent copy is Kenny's Bitwarden. op-18 says 'on a fixed rhythm' but none is set or recorded. The Drive credential is in no backup (op-17). Bitwarden needs master password and 2FA.
- **Failure scenario:** House fire: phone and laptop gone, or Kenny unavailable and his father needs the data; nobody can open the backups.
- **Recommendation:** Make op-18 a recurring manual check (every 90 days) in `homelab checks`; a sealed paper emergency sheet away from the house (restic password, Bitwarden and Google recovery codes, pointer to DR_RUNBOOK); write exact rclone remote settings into Layer 3; a doctor probe that `restic.pw` opens host-meta.

### Restores leave stale files behind, restore whole stacks only, and can mix nights

- **Key:** `restore-stale-files-mixed-nights` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** `restic restore <id> --target /` over a non-empty directory never deletes extra files before restic 0.17 (host version unverified). `homelab restore` restores every app of the stack to `latest` (op-10). Repositories are snapshotted in sequence and a `?` early exit stops the loop, so `latest` can pair a newer database with older media.
- **Failure scenario:** Restoring Paperless leaves newer Postgres WAL segments beside restored files and a corrupt database that 'restored successfully'; rolling back one broken Sonarr also rolls back Radarr and Jellyfin.
- **Recommendation:** Move the target aside (`<path>.pre-restore-<ts>`) and restore into an empty dir, or `--delete` on restic 0.17+; `homelab restore stacks/<s> --app <a> [id]`; tag each night's snapshots `--tag run-<ts>` and restore the newest run present in every repository.

### Native tar backups copy live data without quiescing and skip the guards compose backups have

- **Key:** `native-tar-no-quiesce` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** `backup_native` streams tar of live `data_dirs` for almanac, http-switchboard, kyu-runner and inbox; no emptiness guard, no `restic unlock`, ignores a stack's own `retention` (uses `cfg.tiers`). kyu does it right with `backup_from_newest`.
- **Failure scenario:** almanac's database is written mid-tar and the backup holds a torn file.
- **Recommendation:** Check whether almanac and inbox write state at night; give them the kyu pattern or a brief stop around the tar; bring the native path to parity with compose.

### The ZFS 'replica' mirrors mistakes, sits in the same box, and the media libraries have no copy at all

- **Key:** `zfs-replica-mirrors-mistakes` · **Status:** open · **Effort:** S · **Kind:** needs a decision from Kenny (pve)
- **Reviewers:** backup
- **Evidence:** `zfs send -RI ... | zfs receive -F -x mountpoint` destroys on the target what no longer exists on the source; retention used the same engine (now fixed by fix-42); target HDD18TB in the same chassis; the about 13 TB media libraries have no copy (deliberate, written only as 'never created or backed up by this suite').
- **Failure scenario:** An accidental `zfs destroy HDD2TB/<x>` is carried to the replica the next night.
- **Recommendation:** Call it a mirror in the docs; keep a `zfs hold` on a monthly snapshot on the target or avoid `-R` deletion semantics; record the media decision with its re-acquisition cost, dated and signed off.

### A full-host rebuild has never been rehearsed on bare metal; host bootstrap is manual and recovery time unknown

- **Key:** `rebuild-never-rehearsed` · **Status:** open · **Effort:** L · **Kind:** needs a decision from Kenny
- **Reviewers:** devops, backup
- **Evidence:** Host bootstrap is manual and partly undocumented (Proxmox install, bridges, pool import, restic/rclone install, `rclone config`, offline `restic.pw`; DR_RUNBOOK.md:173-175, 412-418). The full-host path was walked on a live host (T12 part 3) with daemon, rclone and network present, never on bare metal. host-meta never drilled (gap-23).
- **Failure scenario:** The first real rebuild is also the first test, and the next F180-class gap is found under stress.
- **Recommendation:** A rehearsal on a nested Proxmox VM or scratch vmid (host-meta plus one compose and one native stack) to measure recovery time; a quarterly rebuild drill of one real stack.

### Nobody notices if the Cloudflare edge changes; the external monitors accept an unguarded 200

- **Key:** `edge-changes-unnoticed` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** network
- **Evidence:** `captured/gateway/cloudflare-*.json` read once on 2026-09-20; nothing re-reads it. External Kuma monitors accept `["200-299","302"]` (`stacks/uptime/kuma-seeder/seed.py:55,95-96`). CLOUDFLARE.md already drifted (`trmnl.kp-soft.dev` missing).
- **Failure scenario:** A dashboard click or a compromised Cloudflare session flips the wildcard Access app to bypass; `fin` and `docs` answer 200 and stay green, and the house is public without any report.
- **Recommendation:** A nightly read-only diff of tunnel config, DNS and Access against the captures using the existing read-only token; external monitors accept only 302 to `cloudflareaccess.com`; a canary that `sp.kp-soft.dev` without token returns 401.

### The flat VLAN 10 /24 has no east-west control except around CT 116; many unauthenticated LAN services

- **Key:** `flat-vlan-no-east-west-control` · **Status:** open · **Effort:** L · **Kind:** code fix Claude can do alone
- **Reviewers:** network
- **Evidence:** All 15 stacks on `vmbr0` VLAN 10, the container subnet, with HA (Home Assistant (VM 101)) and the workstation (the workstation). Only `116.fw` exists. Published on 0.0.0.0 with no or weak auth: Loki 3100, syslog 1514, Grafana 3000 (F6 parked), GoAccess 7880/7881, FlareSolverr 8191 (in a privileged container), pve-exporter 9221, Alertmanager 9093.
- **Failure scenario:** One compromised container anywhere inherits all of the above plus HA's API and the workstation, none of it crossing OPNsense or logged.
- **Recommendation:** Let the orchestrator generate a per-guest firewall from the stack file (inbound: route ports, Prometheus, Kuma, an explicit `consumers:` list; default drop; outbound drop pve). Set GoAccess `--no-query-string`, bind Grafana/Loki/GoAccess narrowly, drop FlareSolverr's host publish.

### Half the internet exposure lives in hand-written route files on CT 104, and route content is never validated

- **Key:** `routes-outside-repo-unvalidated` · **Status:** open · **Effort:** M · **Kind:** live change on a machine (CT 104 (gateway))
- **Reviewers:** network, security
- **Note:** The step-22 retirement of almanac's hand-written route is a separate item, fixed by fix-41.
- **Evidence:** Not in the repo: `manual-routes.yml` (opn, prox), `manual-homeassistant.yml`, `manual-terminus.yml`, `112-app-almanac.yml` (INVENTORY.md:105-109); kyu's route location unrecorded. `check_gateway_route` / `manifest.rs:848` validate only the filename; no Host uniqueness, no backend-is-own-IP check, nothing stops a management-network backend. Duplicate routers across files caused F115.
- **Failure scenario:** The repo cannot answer 'what is on the internet?', and a stack file can publish any hostname to any internal address without objection.
- **Recommendation:** Bring every route into a stack (`stacks/almanac/traefik-routes.yml`, `stacks/kyu/...`, ha/trmnl in a gateway-owned file with an explicit external backend); plan-time validation (unique Host, backend = own IP unless `external: true`, no management-network backend without a flag); a nightly check listing unowned route files.

### Step-22 route retirement would delete almanac's hand-written route file `112-app-almanac.yml`

- **Key:** `route-retirement-deleted-hand-written` · **Status:** FIXED (fix-41) · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** network
- **Evidence:** `deploy.rs:1966-2010` (built 2026-09-27) deleted `<vmid>-app-<stack>.yml` for any stack without a `gateway_route`; almanac's hand-written route has exactly that name. REGISTER fix-41: the file was still there when checked.
- **Failure scenario:** The next `homelab deploy stacks/almanac` would have removed `almanac.kp-soft.dev` silently.
- **Recommendation:** Fixed by fix-41 part 1: state records the route file a deploy wrote (`StackState.route_file`) and only that file is retired (tests `a_hand_written_route_file_is_never_retired`, `a_deploy_records_the_route_file_it_wrote`).

### Internet-facing Traefik holds the docker socket only to read one middleware label

- **Key:** `traefik-docker-socket` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 104 (gateway))
- **Reviewers:** network, security
- **Note:** network rated MEDIUM, security LOW.
- **Evidence:** `stacks/gateway/traefik/docker-compose.yml:47` mounts `/var/run/docker.sock:ro` (`:ro` does not restrict the API); the docker provider only supplies the `crowdsec-bouncer` middleware (gap-7); Traefik runs a third-party Yaegi plugin on every request.
- **Failure scenario:** A Traefik or plugin RCE becomes root in CT 104, which holds the tunnel token, the bouncer key, Grafana and Loki (limited to the CT because it is unprivileged).
- **Recommendation:** Define the middleware in the file provider (with `{{ env }}` for the LAPI key), use `crowdsec-bouncer@file`, drop the docker provider and the socket; this also closes gap-7's start-up race.

### Verbs disagree on `stacks/<name>` versus `<name>`, and half of them depend on the working directory

- **Key:** `cli-path-vs-name-and-cwd` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `deploy`, `backup`, `restore`, `update`, `resize`, `plan`, `adopt`, `destroy`, `install-native` take a path; `enable`, `disable`, `forget`, `wipe`, `backup-native`, `update-native`, `release-update-native` take a name; wrong form gives hintless errors (`client/src/spec.rs:87-89`, `core/src/ops/enable.rs:41-43`). deploy/apply/new/presets/runbook/testplan/TUI read `./stacks`; from `/tmp`, `check` warns and checks half the fleet (seen live).
- **Failure scenario:** `homelab deploy almanac` answers 'cannot read almanac/lxc-compose.yml' with no hint, and verbs silently work on half the fleet outside the repo.
- **Recommendation:** Normalise `stacks/x`, `x`, `x/` everywhere; find the repository once (walk up for `config/client.toml`, or `repo = ...` in `~/.config/homelab/env`).

### Single TUI keys with lasting consequences and no confirmation

- **Key:** `tui-single-keys-no-confirm` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `e` toggles park at once (`model.rs:845-858`), skipping backups and clearing onboot; `u` starts a host self-update (`:780-792`) next to SHIFT+U; `q` quits during a running operation or with unsaved settings (`:714`); settings `d` deletes a retention tier. Code-derived.
- **Failure scenario:** Pressing `e` instead of `r` parks a stack: no nightly backup and no start after a power cut, shown only as a small `[OFF]` badge.
- **Recommendation:** y/N confirmation on `e`, `u`, settings `d`, with the consequence stated; `q` asks when an op runs or settings are unsaved; move host update into the palette only.

### `check` output buries its one real problem and mixes history into the remedies

- **Key:** `check-output-buries-problem` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** Live: header `9 finding(s)` in red; two are `[noted]` ('nothing to do'); sorted by subject (`manualchecks.rs:176`), so the only `[broken]` is 7th of 9; whole block red; remedies carry history ('(F163)').
- **Failure scenario:** Kenny scans nine red lines and does not see that only one needs action.
- **Recommendation:** A summary line first ('1 broken · 6 to confirm · 2 noted'), then groups by severity, each in its own colour; remedies are commands, the why goes to the docs.

### Long silent waits: `check` 41 s, `doctor` 24 s, and commands during the backup hour wait with no message

- **Key:** `long-silences` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** Live: `check` 41 s and `doctor` 24 s with nothing after 'link up'; the single op lock makes any mutating command wait for the whole nightly batch with no client message (`host/src/main.rs:2981`, runbook op-0).
- **Failure scenario:** A command typed at 02:30 hangs silently for an unknown time.
- **Recommendation:** A progress line per phase; when the lock is taken, report 'waiting for the nightly backup (started 04:00, stack 5 of 14)' at once.

### An older client talking to a newer host gets no warning, and updating the client is a manual chore per machine

- **Key:** `older-client-no-warning` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** The version gate refuses only a client newer than the host (`client/src/version.rs:33-47`, `main.rs:1343-1356`); a stale client may drop fields the host now treats as 'no longer declared, remove' (ask-8). Client update is a 5-line routine per workstation (op-9 step 6). The TUI advertises only host updates.
- **Failure scenario:** A stale client on Garuda deploys to a newer host and drops a field that now means 'remove', the mirror image of the 2026-08-31 data_mounts incident.
- **Recommendation:** Warn and refuse mutating commands when the client is older; `homelab self-install [tag]`; show 'client vX, host vY' in the TUI header.

### The TUI is not calm by default: full effects, scrambling titles, flicker, and alerts only in a scrolling ticker

- **Key:** `tui-not-calm` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `FxLevel::Full` at every launch, not saved (`model.rs:299`); titles glitch (`view/mod.rs:158,545,554,879`, `view/focus.rs:36`); flicker on every tab switch; a 4 s splash (`model.rs:403`); alerts only in a scrolling ticker (`view/mod.rs:596-660`).
- **Failure scenario:** Kenny has to wait for an alarm to scroll past, while panel titles scramble into glyphs.
- **Recommendation:** Default to Subtle or Off and save F2's choice; never animate meaningful text; alerts as a static list.

### TUI indicators claim more than they know, and help and palette have gaps

- **Key:** `tui-indicators-claim-too-much` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `safety whitelist ✓ hostname-guard ✓ fail-closed ✓` is a hard-coded constant (`view/stacks.rs:123-126`); 'drift none' shown in green when drift was never computed (`model.rs:494-509`); `[UPD]` vs SHIFT+U mean different things; `h` map says 1-4 for six tabs; palette lacks deploy, plan, park, checks, apply; Dutch host-question window in an English UI (`view/focus.rs:138-142`).
- **Failure scenario:** A stack with no local files shows a green 'drift none' although nobody compared anything.
- **Recommendation:** Show only what is measured; 'unknown (no local files)'; rename `[UPD]` to `[CHANGED]`; generate key map, footer and palette from one table; one language per surface.

### Help is one flat, ID-laden list of 38 verbs with wrong signatures, and a mistyped verb exits 0

- **Key:** `help-flat-ids-typo-exit0` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** Live `homelab --help`: 38 ungrouped lines with bare IDs (`(E1)`, `(D9/B6)`, `(ask-8)` ...); `export|import <file>` is wrong (`main.rs:437-481`); `destroy --no-backup` and `release-update [tag]` missing; no per-verb help; `homelab stauts` prints help and exits 0 (`main.rs:1138-1204`).
- **Failure scenario:** A typo in a script or a hand-typed command looks like success.
- **Recommendation:** Group the help as README does, drop the IDs, per-verb help with one example, unknown verb prints a suggestion and exits 2.

### Changes reach production without passing CI and without provenance

- **Key:** `changes-reach-prod-without-ci` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** devops
- **Evidence:** `deploy`/`apply` build from the working tree with no git status check (`client/src/main.rs:692-830`); intent commits say only `deploy <stack>` (`core/src/ops/deploy.rs:1254`); binaries report only `CARGO_PKG_VERSION` (`host/src/main.rs:36`); `make install` and `make host-binary` report the same version as the release; branch protection `enforce_admins=false`, no PR requirement (`gh api`).
- **Failure scenario:** A stack file or host binary is live that exists in no commit; 'v3.59.3' names two different binaries; 'which commit is deployed on CT 104?' has no answer.
- **Recommendation:** Embed `git describe --dirty` in both binaries and show it in `/api/version`, `status`, TUI; send HEAD SHA and dirty flag in `DeploySpec` and record it; warn or refuse on uncommitted stack dirs without `--dirty`; fix DEVELOPMENT.md's CI table.

### The local gate skips the tests that validate stack files

- **Key:** `local-gate-skips-stack-tests` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** devops
- **Evidence:** `.claude/hooks/gates.sh:41-44` keys the suite on `*.rs`, `Cargo.toml`, `Cargo.lock` only, while `core/tests/stack_files_tests.rs`, the stale-DR-runbook test (`client/tests/tui_snapshot_tests.rs:861-865`) and `repo_config_tests.rs` validate non-Rust inputs.
- **Failure scenario:** A broken stack YAML is committed and deployed from the working tree before CI ever sees it; the stale-runbook test does not fire on the commit that makes it stale.
- **Recommendation:** Add `stacks/**`, `templates/**`, `presets/**`, `config/**`, `docs/DR_RUNBOOK.md`, `proto/**` to the glob, or invert it.

### `make release`'s 'refuse a red base' guard does not wait, so it rarely has a verdict

- **Key:** `make-release-guard-no-wait` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** devops
- **Evidence:** Measured with `gh run list`: v3.59.3 pushed 14:25:11 while CI ran 14:23:41 to 14:25:22; v3.59.2 the same; pattern holds across the day's releases. `Makefile:73-84` continues on 'no CI verdict'; `git push origin HEAD --follow-tags` (`:127`) releases from any branch; the MSRV message blames the code when the toolchain is missing.
- **Failure scenario:** The guard written after almanac's four red days is bypassed by the normal commit-push-release rhythm.
- **Recommendation:** Wait for pending runs on HEAD (`gh run watch --exit-status`, with timeout) and refuse on red; refuse unless on `main`; check `rustup toolchain list` for the MSRV message.

### Shell scripts are built from strings with non-uniform quoting, and paths are not validated for `'` or `..`

- **Key:** `shell-strings-quoting` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** 117 `pct_sh` and 144 `Cmd::new` calls; three quoting helpers; about 20 sites insert values between bare `'{}'` (e.g. `deploy.rs:447`, `backup.rs:496`, `facts.rs:357`). `storage.host_path` checked only with `starts_with("/appdata/")` (`manifest.rs:749`), `data_mounts.host_path` with `starts_with('/')` (`:685`). Output parsed through magic tokens (`contains("reated")`, `deploy.rs:1696`); 14 probes `.unwrap_or(false)`; `let _ = pct_sh("rm -rf ...")` logs 'removed' anyway (`deploy.rs:2197`).
- **Failure scenario:** A typo such as `/appdata/x'` or `/appdata/../etc` validates and ends up in a root shell command on the Proxmox host.
- **Recommendation:** Validated newtypes (`AppdataPath`, `AppName`, `StackName`) refusing `'`, `..` and empty segments; one `shq` and a `Script` builder whose only interpolation quotes; JSON output parsed with serde; probe failure as a named `Unknown` state.

### `MockExecutor` is growing into a hypervisor simulator, and negative substring asserts can pass without testing anything

- **Key:** `mock-executor-weak-assertions` · **Status:** open · **Effort:** L · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `executor.rs:285-568`, about 280 lines, `pub`, compiled into the host binary; compose model depends on quoting (`rendered.split('\'').nth(1)`, `:436`). 111 negative `calls_containing(x).is_empty()` asserts; an unmatched `respond_always` silently falls back to success.
- **Failure scenario:** If `pct set` is rendered differently, every 'never ran pct set' test still passes.
- **Recommendation:** Move the mock behind a `test-support` feature or dev crate; record calls structurally and assert on verbs; fail on unused expectations; give each negative assert a positive twin; extend the `real_deps_tests.rs` pattern.

### Host config has four representations kept in step by tests that scrape the source text

- **Key:** `config-four-representations` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `FileConfig` (main.rs:59), `Config` (:169), `Out` in `render_settings_toml` (:524), `HostConfigView`; two tests `include_str!("main.rs")` and scrape field names (`:738-800`).
- **Failure scenario:** A formatting change or a nested struct breaks the scraping tests, or a new field is silently not written back.
- **Recommendation:** One serde struct; detect unknown keys with `serde_ignored` or `deny_unknown_fields`; write back by serialising it (`toml_edit` if comments must survive).

## Low (33)

### 'Frozen' architecture decisions the code never implemented; protocol version never bumped

- **Key:** `architecture-decisions-not-built` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, rust
- **Evidence:** AR5 envelope (defined, unused), AR8 minijinja (no dependency), AR15 JSONL ring (none), AR16 frame capture (none), AR18 MSRV CI job (none since d318ada, gap-30) (`ARCHITECTURE_REFERENCE.md` §7). `PROTO_VERSION = 1` never bumped; `Envelope`/`Topic` (`proto/src/lib.rs:385-430`) unused; only 2 structs use `deny_unknown_fields`.
- **Failure scenario:** A reader trusts the architecture document and designs against features that do not exist.
- **Recommendation:** Amend the decisions to match the code or build them; delete `Envelope` or use it; bump `PROTO_VERSION` on wire changes; consider `deny_unknown_fields` on `DeploySpec`.

### `StackState` keeps both legacy `native` and `natives`

- **Key:** `legacy-native-field` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system
- **Evidence:** `state.rs` `migrate_natives`; the deploy carries both (`deploy.rs:2640-2658`).
- **Failure scenario:** Two fields for one fact can disagree after a partial write.
- **Recommendation:** Migrate once and drop `native` in schema v2.

### `applied_hash` is an unkeyed hash over env values stored in a 0644 file and returned by GetState

- **Key:** `applied-hash-secret-derived` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system
- **Evidence:** 64 bits of SHA-256 over manifest, files and env values (`core/src/manifest.rs:951-974`), in `state.json` written 0644 (`state.rs:272-277`).
- **Failure scenario:** Anyone who can read state.json can guess a low-entropy env value offline.
- **Recommendation:** HMAC with a host secret, or hash env separately.

### Production is the test environment: many host releases per day with no staging

- **Key:** `release-churn-no-staging` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny
- **Reviewers:** system, devops
- **Note:** Reviewers counted 16 and 14 releases for the same day.
- **Evidence:** On 2026-09-27: 16 tags by `git tag --sort=creatordate` (system) or 14 host releases v3.58.0 to v3.59.3 (devops); fix-40 needed three releases in 25 minutes, each verified against live containers; every host release restarts the daemon that owns the nightly round.
- **Failure scenario:** A deploy-logic bug is found on live containers; a host rollout inside the backup hour skips or interrupts the night.
- **Recommendation:** A second `homelab-host` against a scratch vmid range or nested PVE; deploy the drill stack with the candidate binary for deploy.rs changes; batch host releases, ship client-only releases without host rollout, avoid the backup hour.

### A daemon restart or power cut during the backup hour skips the whole night

- **Key:** `restart-skips-night` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** system, devops
- **Evidence:** `backup_due` requires `local_hour == backup_hour` (`host/src/main.rs:1952-1954`); ticks every 20 min and the first check waits 20 min (`:2260`). The backup-age finding catches it after 48 h.
- **Failure scenario:** A self-update at 02:10 means no backups, updates, host-meta or fleet check that night.
- **Recommendation:** 'Due and not yet run since the last window' with a catch-up window (e.g. until 06:00), and check once immediately at start-up.

### fix-32's class is still open: API keys in curl argv via `-H` and URL query in about 25 places

- **Key:** `credentials-in-curl-argv` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** security, logging
- **Evidence:** `core/src/ops/busy.rs:167` (`-H "Authorization: MediaBrowser Token=$K"`); `stacks/media/{sonarr,radarr,prowlarr,recyclarr}/checks.yml`, `stacks/syncthing/syncthing/checks.yml` (`-H "X-Api-Key: $K"`, about 20 lines); `stacks/media/jellyfin/checks.yml:46-85` (`?api_key=$K`, also lands in Jellyfin's request log). The guard knows only `-u`/`--user` (`core/tests/argv_secret_tests.rs:36-48`).
- **Failure scenario:** Keys are readable via `/proc/<pid>/cmdline` inside the LXC while curl runs (small exposure, CT root only); the documented invariant 'credentials never in argv' is false.
- **Recommendation:** Widen the detector to `$VAR` in `-H` values and URL queries; convert to the fix-32 `curl -K -` pattern; use the Jellyfin header instead of `api_key` in URLs.

### fix-37/fix-39 residue: a stale flat vault file with a live kyu token, and old token lines in the pve journal

- **Key:** `stale-secret-residue-on-pve` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (pve)
- **Reviewers:** security, logging
- **Evidence:** REGISTER fix-37: flat `secrets/kyu/token.env` left on pve with a live token nothing restores. logging: pve journal still holds 23 `KYU_TOKEN=` lines and 2026-09-01 env dumps (from REGISTER fix-39); tokens not rotated by Kenny's decision; journal retention on pve unmeasured; journal readable by `adm`/`systemd-journal`. Client-side transcripts from before v3.58.8 not searched by logging; security found no live values in `~/.claude/projects/**/*.jsonl`.
- **Failure scenario:** The exposure window of those tokens is open-ended because pve's journal retention is unknown.
- **Recommendation:** Remove the stale flat vault file; vacuum the journal or measure its retention; reconsider rotation.

### Incident bundles and `audit.log` are world-readable (0644) on pve

- **Key:** `bundles-audit-world-readable` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** security
- **Evidence:** `core/src/incidents.rs:68-107` writes bundle files 0644; `audit.log` default mode records full exec commands (`host/src/main.rs:4011-4025`). Bundles carried secrets until masked on 2026-09-27.
- **Failure scenario:** Any local account on pve reads exec commands and bundle transcripts.
- **Recommendation:** 0600 files and 0700 for `incidents/`.

### Unauthenticated `/api/health` and `/api/version`; plaintext bearer from pve to kyu over VLAN 10

- **Key:** `unauth-version-plaintext-kyu` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** security, network
- **Evidence:** `host/src/main.rs:1864-1865` (no token); notification bearer goes to kyu over plain HTTP; cloudflared tunnel token visible via `docker inspect` (root only, acceptable).
- **Failure scenario:** Version disclosure to any neighbour; an ARP-spoofing container sniffs the kyu publish token.
- **Recommendation:** Put `/api/version` behind auth; TLS to kyu or keep the traffic off the shared segment.

### `homelab doctor` checks nothing about security, host-meta, drill or Drive quota, and says Ok for a stack that backs up nothing

- **Key:** `doctor-checks-too-little` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** security, backup
- **Evidence:** Live doctor output: backups and Drive token only; `registry backup — last backup 12h ago` Ok for a `no_backup` stack. No line for listen address, `exec_enabled`, file modes, privileged CTs, unowned route files, failed-auth count, host-meta, drill, password file or quota. rclone `use_trash` may keep pruned packs counting against quota (verify).
- **Failure scenario:** Drive fills with trashed packs and backups start failing without warning.
- **Recommendation:** Cheap doctor probes for the items above, `rclone about gdrive:`, and 'nothing to back up (declared)' for no-backup stacks.

### Code hygiene: duplicated `step!` macro, dependency duplicates, dead tui-preview crate, a test mutating process env, history-laden comments

- **Key:** `rust-code-hygiene` · **Status:** open · **Effort:** M · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `step!` in 12 md5-identical copies; `tokio-rustls` declared unused in client; hand-written base64 in `host/src/tls.rs:79` though `base64` is a dependency; tungstenite twice; no `[workspace.dependencies]`/`[workspace.lints]`; edition 2021 blocked by `set_var` in `#[tokio::main]` (`client/src/main.rs:84`). tui-preview: 4,500 lines, 0 tests, near-copies of theme/fx. Host test sets `HOMELAB_CONFIG` and a fixed `/tmp` path (`main.rs:845-848`). 745 ticket references in comments; a doc comment on a `use` (`main.rs:2146`).
- **Failure scenario:** Maintenance friction and flaky parallel tests.
- **Recommendation:** One `step!` in `ops/mod.rs`; clean dependencies into workspace tables with `unsafe_code = "forbid"`; remove tui-preview or put it behind `default-members`; use `load_config_from`; keep one invariant sentence in code and move the story to commits/CASEBOOK.

### WebSocket edges: a Ping or Binary frame ends the session; a lagging receiver silently drops messages including Ask

- **Key:** `websocket-edge-cases` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `while let Some(Ok(Message::Text(..)))` (`host/src/main.rs:2766`); `Ok(msg) = log_rx.recv()` ignores `Lagged`.
- **Failure scenario:** A slow client never sees a host question and it times out as unattended.
- **Recommendation:** Handle Ping/Binary/Close explicitly; on `Lagged`, tell the client and resend pending `Ask`s.

### First-boot TLS setup does not survive a power cut

- **Key:** `tls-first-boot-power-cut` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** rust
- **Evidence:** `host/src/tls.rs:31` writes the certificate with plain `std::fs::write`, no fsync; `expect("load tls")` (`main.rs:1880`) crash-loops on an empty cert; key-without-cert fails `create_new` and `expect("tls cert")` crash-loops.
- **Failure scenario:** A power cut during the first boot after a rebuild leaves a daemon that crash-loops until someone deletes the files by hand.
- **Recommendation:** Write both atomically with fsync; on a key-without-cert or empty cert, regenerate or fail with a clear message.

### CI hygiene: no cargo-deny/audit, no server-side secret scan, MSRV and diagrams only local, no release cache

- **Key:** `ci-hygiene-gaps` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** devops, rust, security
- **Evidence:** No `cargo deny`/`cargo audit` in CI (F48: Dependabot alerts unread for weeks); local `check-secrets.sh` matches one shape and is bypassable on a public repo; MSRV 1.88 check only in `make release` (measured passing with `cargo +1.88 check --workspace --locked`); `make diagrams` local only; Dependabot commit prefix lacks a bracketed ID; no cargo cache in `release.yml`.
- **Failure scenario:** A vulnerable dependency or an MSRV break is found only at release time or not at all.
- **Recommendation:** Add `cargo deny check advisories`, gitleaks in CI, a 1.88 `cargo check` job, a cache in release.yml, and a bracketed Dependabot prefix.

### `update_native` copies every native binary each night even when nothing changes

- **Key:** `native-update-copies-binary-nightly` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** devops
- **Evidence:** `core/src/ops/native.rs:1253-1272` copies to `.homelab-prev` before `update_cmd` every night for every service (40 MB for kyu), then deletes it.
- **Failure scenario:** Avoidable I/O and a transient disk spike on CT 109's small rootfs.
- **Recommendation:** Check the release first, as `release_update` does, and preserve only on change.

### UPDATE_POLICY.md disagrees with the stack files and with itself

- **Key:** `update-policy-doc-drift` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** devops, system
- **Evidence:** UPDATE_POLICY.md:22-24 vs :92-99 vs :109-112 on http-switchboard, kyu and almanac; REGISTER F192 ('classes and the fleet disagree in seven places') still `doing`.
- **Failure scenario:** The policy table cannot be trusted to say what the round will do.
- **Recommendation:** Generate the policy table from the stack files like the DR runbook, with a staleness test.

### Loki label issues: F59 closed with a query on a non-existent label; `filename` label carries the container id

- **Key:** `loki-label-hygiene` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** F59 was 'measured' with `{container="loki"}` though Alloy's label is `container_name`; nothing drops Loki's own logs. `loki.source.file` adds `filename` with the 64-hex id, so every recreate opens new streams.
- **Failure scenario:** Stream count grows with every recreate, and the register row F59 states something that was never measured.
- **Recommendation:** `stage.drop` for `container_name="loki"` or accept it and correct F59; `stage.label_drop { values = ["filename"] }`.

### Possible double ingestion (journald and /var/log/syslog) and a logrotate duplicate-entry conflict (verify)

- **Key:** `double-ingestion-logrotate` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (fleet containers)
- **Reviewers:** logging
- **Evidence:** Alloy ships both journald and `/var/log/syslog`; `/etc/logrotate.d/homelab` also claims syslog, messages, auth.log already covered by Debian's rsyslog rule. Not measured: `dpkg -l rsyslog; systemctl status logrotate`.
- **Failure scenario:** Every line lands twice where rsyslog is installed, and logrotate exits non-zero daily.
- **Recommendation:** Measure on one container; drop the overlapping paths from the homelab logrotate rule or from Alloy.

### The Traefik access log and GoAccess expose query-string credentials and are not searchable in Grafana

- **Key:** `access-log-query-tokens` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 104 (gateway))
- **Reviewers:** logging, network
- **Evidence:** Access log is a file (`--accesslog.filepath`) read only by CrowdSec and GoAccess, kept 14 days; Jellyfin and HA clients put `api_key`/tokens in URLs. GoAccess publishes the full log on 7880/7881 with query strings (verify with `grep -c api_key access.log`).
- **Failure scenario:** A neighbour on VLAN 10 reads Jellyfin tokens from the GoAccess report.
- **Recommendation:** GoAccess `--no-query-string`; strip or mask query strings in the access log; bind GoAccess narrowly.

### A failed notification route logs the full webhook URL, whose id fix-25 treated as a secret

- **Key:** `webhook-id-in-warn-line` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** `host/src/main.rs:2932`: `notification route <url> failed`.
- **Failure scenario:** The HA webhook id lands in the pve journal on every failed delivery.
- **Recommendation:** Mask the path in the log line.

### `homelab exec` commands are logged verbatim to audit.log and the journal

- **Key:** `exec-logged-verbatim` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** By design (`host/src/main.rs:4011-4025`); a secret typed into `homelab exec` lands in both.
- **Failure scenario:** Kenny passes a password on an exec command line and it is stored on pve indefinitely.
- **Recommendation:** State it in the USER_GUIDE, and optionally run the command through the masker before logging.

### Changing the log caps restarts every container on the host (no `live-restore`)

- **Key:** `docker-no-live-restore` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** logging
- **Evidence:** `guards.rs` step 1 restarts docker whenever `daemon.json` changes; no `"live-restore": true`.
- **Failure scenario:** A guards change restarts every container on a host, e.g. all media apps mid-stream.
- **Recommendation:** Add `"live-restore": true` to the generated `daemon.json`.

### ZFS statistics are duplicated by every LXC node_exporter (20% of the TSDB)

- **Key:** `zfs-metrics-duplicated` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (fleet containers (template) + CT 113)
- **Reviewers:** grafana
- **Evidence:** Measured: 13,797 `node_zfs_*` series, 12,773 from `role="lxc"` exporters; `node_zfs_zpool_state` on 14 hosts.
- **Failure scenario:** Wasted storage, and any naive zpool rule fires 14 times.
- **Recommendation:** `--no-collector.zfs` for LXC exporters in the template, or a relabel drop.

### The deploy's cleanup could delete a committed dashboard if a stack were named like one

- **Key:** `dashboard-cleanup-name-collision` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** `core/src/ops/deploy.rs:2040-2055` runs `rm -f .../homelab-<stack>.json`; committed `homelab-hosts/disks/containers/errors/overview.json` share the namespace.
- **Failure scenario:** A stack named `overview` deletes the home dashboard.
- **Recommendation:** Reserve those names in validation or use a `stack-<name>` uid prefix.

### Two copies of the committed dashboards, and UI edits that silently disappear

- **Key:** `dashboards-two-sources-ui-edits-lost` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** `captured/gateway/grafana/provisioning/` duplicates the committed dashboards (identical today by `diff -rq`); `dashboards.yaml` sets `allowUiUpdates: true`.
- **Failure scenario:** Kenny edits a panel in the browser and the change vanishes after the next restart.
- **Recommendation:** Delete the captured copy; set `allowUiUpdates: false`; drop the now-unneeded `deleteDatasources`.

### Uptime Kuma coverage gaps and seeder drift

- **Key:** `kuma-coverage-and-seeder-drift` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** No application monitor for supersync (`sp.kp-soft.dev`, public bypass), jobtracker, the registry cache, flaresolverr, CrowdSec. `seed.py` reconciles only addresses, not accepted codes, interval, retries or `notificationIDList`. Nothing watches Kuma itself. Every state change also becomes an almanac calendar event.
- **Failure scenario:** A monitor whose HA notification got unticked stays silent forever; a media outage creates seven calendar events plus recoveries.
- **Recommendation:** Declare the missing monitors, reconcile notification list and accepted codes, scrape Kuma's `/metrics`, decide on calendar events per flap.

### Some `never_decreases` checks will flag normal operations

- **Key:** `never-decreases-false-positives` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** grafana
- **Evidence:** Grafana dashboard count drops when a stack is destroyed; Prometheus `count(up==1)` reads low right after a restart; Alertmanager 'ontvangers' greps `"name":` in an embedded YAML string.
- **Failure scenario:** A normal destroy or restart triggers a deploy question that looks like a regression.
- **Recommendation:** Wait one scrape interval; compare dashboards with the stack list; count receivers with `jq` on `config.original` or `amtool`.

### The restore drill includes stateless native units such as `drillsvc`

- **Key:** `drill-includes-stateless-native` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** backup
- **Evidence:** `drill_repos` adds every native unit, including stateless ones while the drill stack exists.
- **Failure scenario:** That drill night fails on a repository that does not exist.
- **Recommendation:** Skip natives without `data_dirs`.

### Apps that trust 'local' callers may treat every proxied internet request as local (verify)

- **Key:** `apps-trust-local-callers` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 106, CT 105, syncthing CT)
- **Reviewers:** network
- **Note:** network rated this LOW-MEDIUM pending verification.
- **Evidence:** Every proxied request reaches its backend from the gateway (CT 104). Candidates: *arr `AuthenticationRequired=DisabledForLocalAddresses`, qBittorrent subnet whitelist, Syncthing without GUI password (route 'answers 200', `stacks/syncthing/lxc-compose.yml:83`). Not read (needs ssh).
- **Failure scenario:** Passing Access is enough to control Sonarr or Syncthing (which holds the Obsidian vault) without any app login.
- **Recommendation:** Verify the three settings; require app auth for every proxied app regardless of source.

### Small network items: broad TRUSTED_PROXIES, ping entrypoint on all interfaces, dead Tailscale whitelist, apex router anchoring

- **Key:** `network-small-items` · **Status:** open · **Effort:** S · **Kind:** live change on a machine (CT 104 (gateway), CT 116 (kp-soft))
- **Reviewers:** network
- **Evidence:** kp-soft `TRUSTED_PROXIES=198.51.100.0/24`, safe only while `116.fw` admits just .4; Traefik ping `8090:8080` on all interfaces; `insecureSkipVerify` on opn/prox backends; CrowdSec whitelist `100.64.0.0/10` for unused Tailscale; apex `kp-soft.dev` will be a second Access-less name.
- **Failure scenario:** Rolling back CT 116's firewall (one `firewall=0`) silently widens who kp-soft trusts as a proxy.
- **Recommendation:** Narrow `TRUSTED_PROXIES` to `198.51.100.4/32`; publish ping on 127.0.0.1; drop the Tailscale range; fix CrowdSec before the apex goes public and Host-anchor the apex router.

### Kenny's workstation (the workstation) lives in the server VLAN

- **Key:** `workstation-in-server-vlan` · **Status:** open · **Effort:** M · **Kind:** needs a decision from Kenny (OPNsense + workstation)
- **Reviewers:** network
- **Evidence:** The workstation is on VLAN 10 with every container (vmid 110 reserved for it).
- **Failure scenario:** A browser compromise on the workstation is adjacent to every server, and a container compromise is adjacent to the workstation.
- **Recommendation:** Move the workstation to its own VLAN now that asymmetric routing is understood (gap-13).

### Colour escape codes are always printed, even when piped

- **Key:** `colour-codes-when-piped` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `C_RED`/`C_GREEN` hard-coded (`client/src/main.rs:24-29`), no `NO_COLOR` or TTY check; captured `check` output full of `\x1b[31m`.
- **Failure scenario:** grep over saved output and logs breaks on escape codes.
- **Recommendation:** Colour only when stdout is a terminal and `NO_COLOR` is unset.

### Small sharp edges: template-build parse fallbacks, `new` demands a token, unhelpful missing-token error, host settings outside the repo

- **Key:** `small-sharp-edges` · **Status:** open · **Effort:** S · **Kind:** code fix Claude can do alone
- **Reviewers:** usability
- **Evidence:** `template-build` turns an unparsable argument into 999 and version 1 (`main.rs:497-499`, the F309 pattern); `homelab new` is local but demands a token (`main.rs:142-147`); 'HOMELAB_TOKEN is not set' names no remedy; TUI settings are saved to `host.toml` (`host/src/main.rs:4109`), outside the declarative repo and `check`.
- **Failure scenario:** A typo in `template-build` silently builds template 999.
- **Recommendation:** Refuse unparsable arguments; no token for local verbs; name `~/.config/homelab/env` in the error; make host settings declarative before any web UI.

## Noted by reviewers but not scored

- **Notification delivery is not proven end to end** (system): 'delivered' means kyu answered 2xx; a message kyu accepts and then drops still sets `last_notify_ok` (F85, NOTIFICATIONS_INVENTORY row 1). Partly covered by `alert-chain-unwatched`.
- **Quiesce labels are not validated** (system): nothing checks that every database-like image carries `com.homelab.backup.pause` (backup found that every current database app does).
- **Grafana has no contact point by decision** (grafana, agrees): log-based alerting therefore does not exist; a Loki ruler is the cheap way back once `container-logs-missing-in-loki` is fixed.

## Where reviewers disagreed

- `container-logs-missing-in-loki`: logging rated this HIGH from code alone (no live Loki read); grafana measured the outage in Loki and rated it CRITICAL.
- `single-offsite-copy-no-integrity-check`: backup rated CRITICAL; security MEDIUM (finding 12, framed as 'no immutable copy'); system folded it into its HIGH H1.
- `crowdsec-blind-to-internet`: network rated HIGH, security MEDIUM (pending the one measurement).
- `admin-uis-on-internet-broad-access`: network rated HIGH, security MEDIUM.
- `nightly-report-always-red`: Reviewers counted the open items differently (system: seven open; usability: six to confirm, ten answers reopened); both agree on 9 findings total.
- `no-pre-update-snapshot`: system folded this into its HIGH H2; devops rated it MEDIUM (M1).
- `state-writes-race`: system rated HIGH, rust MEDIUM.
- `tui-connection-skips-guards`: rust rated HIGH; security rated the TOFU part MEDIUM (finding 9).
- `secret-mask-too-narrow`: logging rated HIGH (measured); security and rust rated LOW.
- `manual-images-latest-unpinned`: system and devops rated MEDIUM, grafana LOW.
- `compose-policy-first-container`: system bundled this in its HIGH H5; devops rated it LOW.
- `compose-update-verify-weak`: system bundled this in its HIGH H5; devops rated it MEDIUM.
- `traefik-docker-socket`: network rated MEDIUM, security LOW.
- `release-churn-no-staging`: Reviewers counted 16 and 14 releases for the same day.
- `apps-trust-local-callers`: network rated this LOW-MEDIUM pending verification.
