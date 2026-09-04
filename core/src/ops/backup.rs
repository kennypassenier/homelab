//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::{Level, PipelineEvent};

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// Where restic keeps its index cache. Without it every single operation
/// re-downloads the repository index from Google Drive first.
///
/// It was not missing by choice: restic derives the path from `$XDG_CACHE_HOME`
/// or `$HOME`, and a systemd service has neither, so every backup in this
/// fleet has run with `unable to open cache: neither $XDG_CACHE_HOME nor
/// $HOME are defined` in its output — a line that reads as noise and costs a
/// full index fetch per repository, of which the gateway alone has six.
pub const RESTIC_CACHE_DIR: &str = "/var/lib/homelab/restic-cache";

/// What one stack's nightly backup did.
///
/// The third state is the point. A backup that stood aside because somebody
/// was watching television did not run — so no timestamp may be recorded, or
/// the staleness check goes quiet about a backup that never happened — and
/// did not fail — so H8 must not park the stack, or the house gets punished
/// for using its own services. A `bool` can only say one of those two wrong
/// things, which is why the deferral needed a state of its own (F280).
///
/// The verdicts live here rather than in the scheduler because they are the
/// decision, and the scheduler is the I/O around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NightBackup {
    Done,
    Deferred(String),
    Failed,
}

impl NightBackup {
    /// Read an operation's outcome. `deferred` is only ever set together with
    /// a false `ok`, but this does not depend on that: a report claiming both
    /// counts as done, because something did run.
    pub fn of(ok: bool, deferred: Option<&str>) -> Self {
        if ok {
            NightBackup::Done
        } else if let Some(why) = deferred {
            NightBackup::Deferred(why.to_string())
        } else {
            NightBackup::Failed
        }
    }

    /// H8: does this night park the stack? Only a real failure does.
    pub fn parks_the_stack(&self, update_ok: bool) -> bool {
        matches!(self, NightBackup::Failed) || !update_ok
    }

    /// May a `last_backup` timestamp be written? Only for work that happened.
    pub fn records_a_timestamp(&self) -> bool {
        matches!(self, NightBackup::Done)
    }

    /// T5: services sharing one container share a fate — the stack's night is
    /// as bad as its worst service. A failure outranks a deferral outranks a
    /// completed backup.
    pub fn worse_of(self, other: NightBackup) -> NightBackup {
        match (self, other) {
            (NightBackup::Failed, _) | (_, NightBackup::Failed) => NightBackup::Failed,
            (d @ NightBackup::Deferred(_), _) => d,
            (_, other) => other,
        }
    }
}

/// Build a Cmd that runs restic with the repo env inline (via `env`).
fn restic(base: &str, stack: &str, password_ref: &str, args: &[&str], timeout: u64) -> Cmd {
    // The host wraps this so RESTIC_PASSWORD comes from its secret store; here
    // we pass a reference the host resolves. In tests the MockExecutor just
    // records the argv. Path join uses "/" — everything lives under one
    // gdrive folder (homelab-backups), not loose dirs in the drive root.
    let repo = format!("{}/{}-config", base, stack);
    let mut full = vec![
        "env".to_string(),
        format!("RESTIC_REPOSITORY={}", repo),
        format!("RESTIC_PASSWORD_FILE={}", password_ref),
        format!("RESTIC_CACHE_DIR={}", RESTIC_CACHE_DIR),
        "restic".to_string(),
    ];
    full.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    Cmd::new(refs[0], &refs[1..], timeout)
}

#[derive(Clone)]
pub struct BackupCfg {
    pub restic_base: String,
    /// Path to the restic password file on the host (from the secret store).
    pub password_file: String,
    /// Tiered retention (G8) — computed by us, not restic's --keep-* flags.
    pub tiers: Vec<crate::retention::RetentionTier>,
    /// Snapshot timeout. Hardening H2: the old fixed 1800 s was too small
    /// for a first multi-GB upload over residential rclone/gdrive.
    pub snapshot_timeout_s: u64,
    /// Restore timeout. Was a hardcoded 1800 s while the backup side had
    /// already been raised to four hours for exactly the same reason — so a
    /// large restore over Google Drive died at thirty minutes, on the one
    /// operation you least want to find broken (deployment project, F38).
    pub restore_timeout_s: u64,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab-backups".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            tiers: crate::retention::default_tiers(),
            snapshot_timeout_s: 4 * 3600,
            restore_timeout_s: 4 * 3600,
        }
    }
}

/// Build a restic command from a BackupCfg (shared with deploy's E3
/// auto-restore step).
pub(crate) fn restic_cmd(cfg: &BackupCfg, stack: &str, args: &[&str], timeout: u64) -> Cmd {
    restic(&cfg.restic_base, stack, &cfg.password_file, args, timeout)
}

/// The newest snapshot across a stack's per-app repositories, or None when
/// nothing answered. The repository is the truth about when a stack was last
/// backed up; `StackState::last_backup` is only a cache of it, and a C4
/// replacement throws that cache away with the container it destroys.
///
/// Found by the M7 drill (2026-08-31): CT 115 was backed up twelve minutes
/// before it was replaced, came back reporting it had never been backed up,
/// and the fleet check dutifully called it broken while the snapshot sat in
/// the repository untouched.
pub(crate) async fn newest_snapshot_unix(
    exec: &dyn Executor,
    m: &StackManifest,
    cfg: &BackupCfg,
) -> Option<u64> {
    let mut newest: Option<u64> = None;
    for (owner, _paths) in owner_groups(m) {
        // A repository that does not exist yet is the normal case for a new
        // stack, so a failure here is silence, not an error.
        let Ok(out) = exec
            .run(&restic_cmd(
                cfg,
                &owner,
                &["snapshots", "--latest", "1", "--json"],
                120,
            ))
            .await
        else {
            continue;
        };
        if !out.success() {
            continue;
        }
        if let Some(t) = parse_snapshots_json(&out.stdout)
            .into_iter()
            .map(|(_, t)| t)
            .max()
        {
            newest = Some(newest.map_or(t, |n: u64| n.max(t)));
        }
    }
    newest
}

/// D25: group the manifest's storage paths by the app that owns them, in
/// manifest order. A path with no declared owner belongs to the stack, which
/// keeps host-level paths (and every manifest written before the field
/// existed) working exactly as they did.
/// Public because the disaster-recovery runbook must name the SAME
/// repositories the backup actually writes to. It used to derive them itself,
/// from the stack name, and so printed `media-config` for a stack whose
/// repositories are `jellyfin-config`, `sonarr-config`, `radarr-config` and
/// three more. That document is read exactly once — when everything else is
/// gone — and it would have said the backups were not there.
/// D25 names a restic repository after the OWNING APP, not the stack — so an
/// app that moves between stacks keeps its history. The other side of that
/// coin: two stacks that name the same owner share one repository, and
/// nothing said so.
///
/// Found by running the G13 drill (F285). A throwaway stack called `drill`
/// declared a native unit `http-switchboard`, which is also the name of a live
/// service on CT 109. Its destroy took the mandatory backup-before-destroy —
/// into `http-switchboard-config`, the live repository — and then applied that
/// throwaway stack's retention to it, which DELETED the real service's most
/// recent snapshot. The drill's own snapshot was then `latest`, so a restore
/// of the real service would have handed back the drill's fake configuration.
///
/// Returns the first conflict as `(owner, the other stack)`. Nothing is
/// reported for the stack's own name, nor for an owner no other stack claims.
pub fn conflicting_owner(
    stack: &str,
    owners: &[String],
    others: &[(String, Vec<String>)],
) -> Option<(String, String)> {
    for owner in owners {
        for (other_stack, other_owners) in others {
            if other_stack == stack {
                continue;
            }
            if other_owners.iter().any(|o| o == owner) {
                return Some((owner.clone(), other_stack.clone()));
            }
        }
    }
    None
}

pub fn owner_groups(m: &StackManifest) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for mount in &m.storage {
        // An app that declares it keeps nothing gets no repository at all —
        // there is then nothing for the empty-snapshot guard to refuse, and
        // nothing that can stop the rest of the stack (F154, Kenny's B4).
        if mount.no_data {
            continue;
        }
        // Z3: declared reproducible, so no repository either. The difference
        // from `no_data` is what it means, not what it does here: that one
        // holds nothing, this one holds something nobody needs to keep. The
        // reason travels with the flag and is surfaced by the fleet check and
        // the runbook, because a directory that is silently unprotected and
        // one that is deliberately unprotected must not look the same.
        if mount.no_backup.is_some() {
            continue;
        }
        let owner = mount.owner(&m.stack_name).to_string();
        match groups.iter_mut().find(|(o, _)| *o == owner) {
            Some((_, paths)) => paths.push(mount.host_path.clone()),
            None => groups.push((owner, vec![mount.host_path.clone()])),
        }
    }
    groups
}

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
pub async fn backup(ctx: &OpCtx<'_>, m: &StackManifest, cfg: &BackupCfg) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    // D25: one repository per owning app, so an app that moves to another
    // stack keeps its history. Order is the manifest's, so the log reads the
    // way the file does.
    let groups = owner_groups(m);

    // A1/A2: same gate as every mutating op (quiesce/resume reach into the
    // container).
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // F285: does another stack already own one of these repositories?
    //
    // The repository is named after the owning APP (D25), so two stacks that
    // name the same owner write into one repository — and the retention pass
    // that follows a snapshot then applies THIS stack's tiers to the other
    // stack's history. The G13 drill did exactly that and deleted a live
    // service's most recent backup.
    //
    // Fail closed and before anything runs: a backup that quietly writes into
    // somebody else's repository is worse than no backup, because the
    // repository still looks healthy afterwards.
    step!(runner, "owner conflict", {
        let store = crate::state::StateStore::new(ctx.exec, &ctx.state_dir);
        let Ok(snapshot) = store.load().await else {
            // No state file is a first deploy, not a conflict.
            return Ok(StepOutcome::Unchanged);
        };
        let others: Vec<(String, Vec<String>)> = snapshot
            .stacks
            .iter()
            .filter_map(|(name, st)| {
                let man = st.manifest.as_ref()?;
                Some((
                    name.clone(),
                    owner_groups(man).into_iter().map(|(o, _)| o).collect(),
                ))
            })
            .collect();
        let owners: Vec<String> = groups.iter().map(|(o, _)| o.clone()).collect();
        if let Some((owner, other)) = conflicting_owner(&m.stack_name, &owners, &others) {
            return Err(CoreError::SafetyAbort(format!(
                "stack '{}' would back up into the repository '{}-config', which stack '{}' \
                 already owns :: repositories are named after the owning app (D25), so both \
                 stacks write into ONE history and the retention pass afterwards applies this \
                 stack's tiers to the other stack's snapshots. Rename the app in one of the \
                 two stack files",
                m.stack_name, owner, other
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // O10, second caller: ask before stopping anything.
    //
    // On 2026-09-04 at 04:17 the nightly round ran `docker stop bazarr
    // prowlarr jellyfin seerr radarr sonarr` on CT 106 while Kenny was
    // watching an episode. It came back thirty seconds later and his player
    // skipped to the next one. The check that exists to prevent exactly this
    // was already written, already correct and already armed — on the UPDATE
    // path. The backup path stops the same containers every single night and
    // never asked (F280).
    //
    // Standing aside is not a failure and not a success. It returns
    // `CoreError::Deferred`, which leaves `ok` false so no backup timestamp
    // is recorded for work that did not happen, and carries `deferred` so the
    // nightly round does not count it as a failed night and park the stack.
    // Tomorrow it runs. If it keeps standing aside, the backup staleness
    // check in `fleetcheck` is what says so — that escalation already exists
    // and does not need a counter here.
    //
    // It is the whole stack that defers, not one app: the apps share a
    // container and the snapshot is taken of the stack's paths in one pass.
    // Backing up five of six configs while the sixth is live would be a
    // partial snapshot nobody asked for.
    step!(runner, "in use?", {
        for app in &m.apps {
            let Some(verdict) =
                // ctx.exec, not the tracing one: see `busy::app_busy`.
                crate::ops::busy::app_busy(ctx.exec, m.vmid, &m.stack_name, app).await?
            else {
                continue;
            };
            if verdict.may_update() {
                continue;
            }
            return Err(CoreError::Deferred(format!(
                "{} is in use, so nothing was stopped and no snapshot was taken: {}",
                app,
                crate::ops::busy::reason(&verdict)
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // The other half of `no_data`: a declaration is only worth having if it
    // is checked. An app that says it keeps nothing and then keeps something
    // has quietly opted its data out of every backup, which is a worse
    // failure than the one the flag was added to fix.
    step!(runner, "declared-empty paths", {
        let mut wrong = Vec::new();
        for mount in m.storage.iter().filter(|s| s.no_data) {
            let out = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "find '{}' -mindepth 1 -maxdepth 1 2>/dev/null | head -5 | wc -l",
                            mount.host_path
                        ),
                    ],
                    60,
                ))
                .await?;
            if out.stdout.trim() != "0" {
                wrong.push(mount.host_path.clone());
            }
        }
        if !wrong.is_empty() {
            return Err(CoreError::Validation(format!(
                "these paths are declared `no_data: true` and are not empty: {} — \
                 nothing in them is being backed up, by declaration. Either the \
                 declaration is stale or something started writing there",
                wrong.join(", ")
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Z5 (Kenny, form Z5): a declared path that does not exist yet is not a
    // broken backup, it is a stack that has not been deployed since its file
    // changed. restic's own answer to it —
    // `Fatal: all source directories/files do not exist` — names the wrong
    // problem, and on 2026-09-02 it cost the author several minutes and a
    // read of the raw log to work out that `stacks/uptime` had simply gained
    // a mount that evening and never been deployed (F170).
    step!(runner, "declared paths exist", {
        let mut missing = Vec::new();
        for mount in m
            .storage
            .iter()
            .filter(|s| !s.no_data && s.no_backup.is_none())
        {
            let out = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!("test -d '{}' && echo yes || echo no", mount.host_path),
                    ],
                    30,
                ))
                .await?;
            if out.stdout.trim() != "yes" {
                missing.push(mount.host_path.clone());
            }
        }
        if !missing.is_empty() {
            return Err(CoreError::Validation(format!(
                "these paths are declared in the stack file and do not exist on the host: {} :: \
                 this is almost always a stack whose file gained a mount and was not deployed \
                 afterwards — `homelab deploy stacks/{}` creates them. It is NOT a broken \
                 backup, and restic's own message for it says the opposite",
                missing.join(", "),
                m.stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "init repos", {
        // Idempotent: init fails harmlessly if the repo already exists.
        for (owner, _) in &groups {
            let _ = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["init"],
                    120,
                ))
                .await?;
        }
        Ok(StepOutcome::Unchanged)
    });

    // H2 hardening: a previous run killed mid-snapshot can leave a stale
    // repo lock; restic unlock only removes locks from dead processes, so
    // this is always safe. Best-effort (repo may not exist yet).
    step!(runner, "clear stale locks", {
        for (owner, _) in &groups {
            let _ = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["unlock"],
                    120,
                ))
                .await;
        }
        Ok(StepOutcome::Unchanged)
    });

    // Quiesce: stop containers labeled com.homelab.backup.pause=true, and
    // REMEMBER WHICH ONES.
    //
    // This used to stop by label and resume by the manifest's `apps` list,
    // and the two are not the same set. On 2026-08-31 the metrics stack's
    // nightly backup stopped prometheus and alertmanager — both labelled —
    // and resumed prometheus, promtail and pve-exporter, because host state
    // still held the app list from before alertmanager was added. The
    // snapshot then failed on a stale path, so nothing else touched the
    // stack, and Alertmanager stayed down for six hours. Nothing reported
    // it; Kenny saw it in Uptime Kuma, which had been watching it for two
    // hours by then.
    //
    // A backup that can leave a service off is worse than a backup that
    // fails, so what is paused is now what is resumed, by name.
    let paused: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let paused_w = paused.clone();
    step!(runner, "quiesce", {
        let script = "docker ps --filter label=com.homelab.backup.pause=true --format '{{.Names}}'";
        let out = super::util_pct_sh(exec, m.vmid, script, 60).await?;
        let names: Vec<String> = out
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if names.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let stop = format!("docker stop {}; true", names.join(" "));
        let _ = super::util_pct_sh(exec, m.vmid, &stop, 120).await?;
        if let Ok(mut g) = paused_w.lock() {
            *g = names;
        }
        Ok(StepOutcome::Changed)
    });

    // H2 hardening: the snapshot may fail, but RESUME MUST ALWAYS RUN — a
    // fail-closed abort here would leave the quiesced databases down until
    // a human noticed. So the snapshot error is captured, resume runs
    // unconditionally, and only then does the operation fail.
    let snapshot_result = runner
        .step("snapshot", || async {
            if groups.is_empty() {
                return Ok(StepOutcome::Unchanged);
            }
            for (owner, paths) in &groups {
                // --quiet as well as --json: without it restic emits a status line per
                // update and the operation log becomes a wall of progress json.
                // Quiet keeps the summary, which is the only line this needs.
                let mut args = vec!["backup", "--quiet", "--json"];
                for p in paths {
                    args.push(p.as_str());
                }
                let out = run_ok(
                    exec,
                    &restic(
                        &cfg.restic_base,
                        owner,
                        &cfg.password_file,
                        &args,
                        cfg.snapshot_timeout_s,
                    ),
                )
                .await?;
                // A restic run over a directory that exists and is empty
                // succeeds, writes a snapshot containing nothing, and reports
                // success. The record then says the stack is backed up and
                // the restore has nothing to give back — the same shape as
                // every other finding here: a green result that proves the
                // wrong thing.
                //
                // A path that does not exist already fails loudly (rc=1), and
                // that is how the metrics stack's stale path was caught on
                // 2026-08-31. An empty one is the case nothing catches.
                if snapshot_is_empty(&out.stdout) {
                    return Err(CoreError::Command {
                        rendered: format!("restic backup {}", owner),
                        detail: format!(
                            "the snapshot for '{}' contains no files :: it covered {} — check the path holds what you think it does, because a restore from this gives back nothing",
                            owner,
                            paths.join(", ")
                        ),
                    });
                }
            }
            Ok(StepOutcome::Changed)
        })
        .await;

    // Resume the paused containers — unconditionally.
    let paused_r = paused.clone();
    step!(runner, "resume", {
        // Exactly what quiesce stopped, by name — this is the half that must
        // not depend on any list that can go stale.
        let names = paused_r.lock().map(|g| g.clone()).unwrap_or_default();
        if !names.is_empty() {
            let start = format!("docker start {}; true", names.join(" "));
            let _ = super::util_pct_sh(exec, m.vmid, &start, 300).await?;
        }
        // Then the declared apps, which also brings back anything that was
        // down for an unrelated reason. Belt and braces: this is the step
        // that runs even when the snapshot failed.
        let dir_cmds = m
            .apps
            .iter()
            .map(|a| format!("cd '/opt/{}/{}' && docker compose up -d", m.stack_name, a))
            .collect::<Vec<_>>()
            .join("; ");
        if !dir_cmds.is_empty() {
            let _ = super::util_pct_sh(exec, m.vmid, &format!("{}; true", dir_cmds), 300).await?;
        }
        Ok(StepOutcome::Changed)
    });

    if let Err(e) = snapshot_result {
        return runner.finish_err("snapshot", &e);
    }

    step!(runner, "retention", {
        // G8 tiered retention: list snapshots, compute the forget-set with
        // our own engine, forget by explicit id. Per repository, since D25
        // gave every app its own.
        //
        // W2: the stack file's own policy wins over the fleet-wide one when
        // it states one. Resolved here rather than where the config is built,
        // so every caller — a manual backup, the nightly run, a future one —
        // gets it without being told to.
        let tiers = m.retention.as_ref().unwrap_or(&cfg.tiers);
        if m.retention.is_some() {
            ctx.sink.emit(PipelineEvent::Line {
                level: Level::Info,
                source: "HOST".into(),
                msg: format!(
                    "[w2] {} keeps snapshots by its own policy ({} tier(s)), not the fleet-wide one",
                    m.stack_name,
                    tiers.len()
                ),
            });
        }
        let mut changed = false;
        for (owner, _) in &groups {
            let out = run_ok(
                exec,
                &restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["snapshots", "--json"],
                    300,
                ),
            )
            .await?;
            let snapshots = parse_snapshots_json(&out.stdout);
            let doomed = crate::retention::forget_list(&snapshots, tiers, ctx.now_unix);
            if doomed.is_empty() {
                continue;
            }
            let mut args: Vec<&str> = vec!["forget"];
            args.extend(doomed.iter().map(|s| s.as_str()));
            args.push("--prune");
            run_ok(
                exec,
                &restic(&cfg.restic_base, owner, &cfg.password_file, &args, 900),
            )
            .await?;
            changed = true;
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    runner.log(
        Level::Info,
        format!("[backup] {} snapshot complete", m.stack_name),
    );
    runner.finish_ok()
}

/// Parse `restic snapshots --json` into `(short_id, unix_time)` pairs.
/// Tolerant of extra fields; returns empty on malformed input (retention
/// then keeps everything — fail-safe direction).
pub(crate) fn parse_snapshots_json(raw: &str) -> Vec<(String, u64)> {
    #[derive(serde::Deserialize)]
    struct Snap {
        short_id: String,
        time: String,
    }
    let Ok(snaps) = serde_json::from_str::<Vec<Snap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps
        .into_iter()
        .filter_map(|s| {
            // RFC3339 → unix without pulling in chrono: date parsing via the
            // subset restic emits (e.g. 2026-08-11T04:00:12.123+02:00).
            humantime_to_unix(&s.time).map(|t| (s.short_id, t))
        })
        .collect()
}

/// Minimal RFC3339 → unix seconds (UTC), no external crates. Handles the
/// forms restic emits; returns None on anything unexpected.
fn humantime_to_unix(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Timezone offset: trailing Z or ±HH:MM after the (optional) fraction.
    let rest = &s[19..];
    let offset_secs: i64 = if rest.ends_with('Z') || rest.is_empty() {
        0
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let sign = if rest.as_bytes()[pos] == b'+' { 1 } else { -1 };
        let tz = &rest[pos + 1..];
        let th = tz.get(0..2)?.parse::<i64>().ok()?;
        let tm = tz.get(3..5)?.parse::<i64>().ok()?;
        sign * (th * 3600 + tm * 60)
    } else {
        0
    };
    // Days since epoch (civil-from-days algorithm, Howard Hinnant).
    let (y, mo) = if mo <= 2 { (y - 1, mo + 12) } else { (y, mo) };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (mo - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let unix = days * 86_400 + h * 3600 + mi * 60 + sec - offset_secs;
    u64::try_from(unix).ok()
}

/// E2: restore a stack's /appdata from a snapshot (default: latest).
/// validate → quiesce → restore → resume → verify.
/// G14: pull one repository's newest snapshot into a scratch directory.
///
/// Deliberately NOT the `restore` below: that one quiesces the stack's
/// containers, writes over live paths and is the operation you run when
/// something is broken. A drill must prove the backup without touching
/// anything that is working, so it restores somewhere harmless and the caller
/// throws the result away.
pub async fn restore_into(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    app: &str,
    target: &str,
) -> Result<(), CoreError> {
    let out = exec
        .run(&restic_cmd(
            cfg,
            app,
            &["restore", "latest", "--target", target],
            cfg.restore_timeout_s,
        ))
        .await?;
    if out.code != 0 {
        return Err(CoreError::Command {
            rendered: format!("restic restore latest --target {}", target),
            detail: format!(
                "restic restore of {} exited {}: {}",
                app,
                out.code,
                out.stderr.trim()
            ),
        });
    }
    Ok(())
}

pub async fn restore(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> OperationReport {
    let op = format!("restore-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    runner.log(
        Level::Warn,
        format!("[restore] {} from snapshot '{}'", m.stack_name, snapshot),
    );

    // A1/A2: restore composes down and writes over the target — full gate.
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // D25: a stack's data lives in one repository per owning app, so a
    // restore walks all of them. Order is the manifest's.
    let groups = owner_groups(m);

    // G5 of the Phase-7 gate: this step was called "validate snapshot" and
    // never looked at the snapshot. It took the caller's id, asked each
    // repository whether it answered at all, and returned — so a typo'd id
    // passed here, the stack was composed down, and restic then failed on
    // something that does not exist. The name promised the check; only the
    // name.
    step!(runner, "validate snapshot", {
        for (owner, _) in &groups {
            let out = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["snapshots"],
                    120,
                ))
                .await?;
            if !out.success() {
                return Err(CoreError::Other(format!(
                    "restic repo for '{}' unreachable",
                    owner
                )));
            }
            // `latest` is restic's own word for "whatever the newest is" and
            // is always valid as long as the repository holds anything.
            if snapshot == "latest" {
                if out.stdout.lines().filter(|l| !l.trim().is_empty()).count() < 2 {
                    return Err(CoreError::Other(format!(
                        "repository for '{}' holds no snapshots at all, so \
                         'latest' means nothing",
                        owner
                    )));
                }
                continue;
            }
            // restic abbreviates ids in its listing, so a prefix match is the
            // honest comparison — and it is what the user typed anyway.
            if !out
                .stdout
                .split_whitespace()
                .any(|w| w.starts_with(snapshot) || snapshot.starts_with(w) && w.len() >= 8)
            {
                return Err(CoreError::Other(format!(
                    "snapshot '{}' is not in the repository for '{}' — nothing \
                     has been stopped",
                    snapshot, owner
                )));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    // Stop the whole stack for a consistent restore.
    step!(runner, "quiesce stack", {
        for a in &m.apps {
            let _ = super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd '/opt/{}/{}' && docker compose down", m.stack_name, a),
                120,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    // G4 of the Phase-7 gate, and the same lesson `backup()` above already
    // carries in capitals: the restore may fail, but RESUME MUST ALWAYS RUN.
    // A fail-closed abort here leaves the stack composed down — after a
    // four-hour timeout on Google Drive, or a dropped connection — until a
    // human notices. That is a self-inflicted outage on the one operation
    // you run when something is already wrong.
    let restore_result = runner
        .step("restore data", || async {
            for (owner, _) in &groups {
                run_ok(
                    exec,
                    &restic(
                        &cfg.restic_base,
                        owner,
                        &cfg.password_file,
                        &["restore", snapshot, "--target", "/"],
                        cfg.restore_timeout_s,
                    ),
                )
                .await?;
            }
            Ok(StepOutcome::Changed)
        })
        .await;

    step!(runner, "resume stack", {
        for a in &m.apps {
            super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd '/opt/{}/{}' && docker compose up -d", m.stack_name, a),
                300,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    if let Err(e) = restore_result {
        return runner.finish_err("restore data", &e);
    }

    step!(runner, "verify health", {
        for a in &m.apps {
            let out = super::util_pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd '/opt/{}/{}' && docker compose ps --status running --services",
                    m.stack_name, a
                ),
                60,
            )
            .await?;
            if out.stdout.trim().is_empty() {
                return Err(CoreError::Other(format!("{} not running after restore", a)));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    runner.log(
        Level::Info,
        format!("[restore] {} restored and verified", m.stack_name),
    );
    runner.finish_ok()
}

/// H10 hardening: snapshot the host's own critical metadata — the secrets
/// vault, state.json, and TLS material — into a dedicated `host-meta` repo.
/// Without this, losing the host disk loses the keys needed for recovery.
pub async fn backup_host_meta(ctx: &OpCtx<'_>, cfg: &BackupCfg) -> OperationReport {
    let mut runner = Runner::new("host-meta-backup", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let secrets = format!("{}/secrets", ctx.state_dir);
    let state_file = format!("{}/state.json", ctx.state_dir);
    let tls_cert = format!("{}/tls-cert.pem", ctx.state_dir);
    let tls_key = format!("{}/tls-key.pem", ctx.state_dir);
    // The intent repo carries every applied compose file plus its git
    // history — cheap to include, and it turns "restore the host" into
    // "restore the host AND know what ran on it".
    let repo = format!("{}/repo", ctx.state_dir);
    // F180: the daemon's own configuration file. It was NOT in this backup
    // until 2026-09-02, which was found by walking the disaster-recovery
    // runbook and comparing what it promises against what is on Google
    // Drive. This repo is called the host's crown jewels and did not contain
    // the file holding the API token, the notify bearer, the OPNsense
    // credential path, the ZFS jobs and every other knob — so a rebuilt host
    // would have had its keys and its history back, and still needed that
    // file retyped from memory before anything could talk to it.
    //
    // Not under `state_dir`, so it is named separately rather than swept up.
    let host_config = "/etc/homelab/host.toml".to_string();

    // F274: the pieces of the Proxmox host that this suite installed and that
    // live nowhere under `state_dir` either. They are in `captured/pve-host/`
    // in the repository and their checksums match the live files, so they do
    // survive a host loss — but by a different route from every other piece
    // of host configuration, and a restore that follows this runbook would
    // put back everything except these three.
    //
    // Absent paths are skipped rather than fatal: a host that never had the
    // SMART collector is not a broken backup, and restic refuses the whole
    // snapshot if any source is missing.
    let host_extras: Vec<String> = [
        "/usr/local/bin/smart-textfile-collector.py",
        "/etc/systemd/system/smart-collector.service",
        "/etc/systemd/system/smart-collector.timer",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    step!(runner, "init repo", {
        let _ = exec
            .run(&restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &["init"],
                120,
            ))
            .await?;
        Ok(StepOutcome::Unchanged)
    });

    // Which of the extras this host actually has. Read once, here, so the
    // snapshot step gets a list that cannot fail on a missing path.
    let mut present: Vec<String> = Vec::new();
    step!(runner, "host extras", {
        for p in &host_extras {
            let out = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", &format!("test -e '{}' && echo yes || true", p)],
                    30,
                ))
                .await?;
            if out.stdout.trim() == "yes" {
                present.push(p.clone());
            }
        }
        ctx.sink.emit(PipelineEvent::Line {
            level: Level::Info,
            source: "HOST".into(),
            msg: format!(
                "[host-meta] {} of {} host extra(s) present: {}",
                present.len(),
                host_extras.len(),
                if present.is_empty() {
                    "—".to_string()
                } else {
                    present.join(", ")
                }
            ),
        });
        Ok(StepOutcome::Unchanged)
    });

    let mut args: Vec<&str> = vec![
        "backup",
        &secrets,
        &state_file,
        &tls_cert,
        &tls_key,
        &repo,
        &host_config,
    ];
    args.extend(present.iter().map(|s| s.as_str()));

    step!(runner, "snapshot", {
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &args,
                600,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        "[host-meta] vault/state/tls snapshot complete".to_string(),
    );
    runner.finish_ok()
}

/// Did the run that produced this output actually store anything?
///
/// restic's `--json` stream ends with a `summary` message carrying the
/// counts. Absence of a summary is NOT treated as empty: a version that
/// changes its output should not turn every backup into a failure — a check
/// that fires on something it merely does not recognise is worse than no
/// check, because it teaches people to ignore it.
pub fn snapshot_is_empty(stdout: &str) -> bool {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') || !line.contains("\"message_type\":\"summary\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let files = v
            .get("total_files_processed")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        let bytes = v
            .get("total_bytes_processed")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        return files == 0 && bytes == 0;
    }
    false
}
