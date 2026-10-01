//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok, shq};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::{Level, PipelineEvent};

use super::OpCtx;

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

    /// May tonight's automatic updates of this stack run? Only after a backup
    /// that happened.
    ///
    /// fix-60: the update waits for a night with a backup; a stack that
    /// keeps standing aside is escalated by the backup-age check, not
    /// updated blind. Story: `docs/deployment/REGISTER.md`.
    pub fn allows_update(&self) -> bool {
        matches!(self, NightBackup::Done)
    }

    /// The log line for a stack whose updates this backup outcome held back.
    pub fn update_skip_line(&self, stack: &str) -> String {
        let why = match self {
            NightBackup::Done => "it ran".to_string(),
            NightBackup::Deferred(why) => format!("it stood aside ({})", why),
            NightBackup::Failed => "it failed".to_string(),
        };
        format!(
            "scheduler: automatic updates of {} skipped tonight — no backup of tonight to go \
             back to: {}",
            stack, why
        )
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
pub(crate) fn restic(
    base: &str,
    stack: &str,
    password_ref: &str,
    args: &[&str],
    timeout: u64,
) -> Cmd {
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

/// What `restic init` found (gap-24).
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    Created,
    Existed,
}

/// Run `restic init` and read its answer, which used to be thrown away.
///
/// "already exists" / "already initialized" is the normal nightly answer.
/// Anything else that fails (a permission error, a dead remote) is an error
/// now: the backup that followed it could only fail less clearly.
pub async fn init_repository(
    exec: &dyn Executor,
    init: &Cmd,
    repo: &str,
) -> Result<InitOutcome, CoreError> {
    let out = exec.run(init).await?;
    if out.code == 0 {
        return Ok(InitOutcome::Created);
    }
    let text = format!("{} {}", out.stderr.trim(), out.stdout.trim());
    if text.contains("already exists") || text.contains("already initialized") {
        return Ok(InitOutcome::Existed);
    }
    Err(CoreError::Other(format!(
        "restic init of {}-config failed: {} :: nothing was backed up; check the remote \
         (`rclone lsd` on the host) and the password file",
        repo,
        text.trim()
    )))
}

/// A repository created tonight must open with the password that opens
/// host-meta-config (gap-24). A regenerated password file would otherwise
/// encrypt every new repository with a key the offline copy cannot open,
/// and nothing would say so until a restore. When host-meta-config does not
/// exist yet (a first night) there is nothing to compare with.
pub async fn same_password_as_host_meta(
    exec: &dyn Executor,
    restic_base: &str,
    password_file: &str,
    created: &str,
) -> Result<(), CoreError> {
    let out = exec
        .run(&restic(
            restic_base,
            "host-meta",
            password_file,
            &["cat", "config"],
            120,
        ))
        .await?;
    let text = format!("{} {}", out.stderr, out.stdout);
    if out.code != 0 && (text.contains("wrong password") || text.contains("no key found")) {
        return Err(CoreError::Validation(format!(
            "{}-config was just created with a password that does not open host-meta-config \
             :: the password file on the host changed. Put back the password the offline copy \
             holds (docs/OPERATIONS_RUNBOOK.md op-17, lost-1), then remove the new repository \
             {}-config before the next run",
            created, created
        )));
    }
    Ok(())
}

/// Rule 20 / coordinator 2026-10-01: staging is on by default, on the same
/// pool as the data it copies (not the root disk), capped by
/// `native_backup_staging_cap_mib` and emptied after every run.
pub const DEFAULT_STAGING_DIR: &str = "/appdata/.backup-staging";

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
    /// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): where a
    /// `backup_pause: chassis` native backup tars its data dirs LOCALLY
    /// before restic uploads them, so the chassis pause only has to last as
    /// long as a disk-to-disk copy — never a slow upload. `None`: always tar
    /// straight to restic under a renewed pause (no staging; the only
    /// behaviour possible before this field existed). A directory on a data
    /// pool, not the root disk — `native_backup_staging_dir` in host.toml.
    pub staging_dir: Option<String>,
    /// Largest one staged tar may be, in MiB, BEFORE the 20% safety margin
    /// `fits_staging` adds on top. A copy that would not fit, after the
    /// margin, against either this cap or the directory's free space skips
    /// staging for that run rather than shrinking the margin to fit.
    pub staging_cap_mib: u64,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab-backups".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            tiers: crate::retention::default_tiers(),
            snapshot_timeout_s: 4 * 3600,
            restore_timeout_s: 4 * 3600,
            staging_dir: Some(DEFAULT_STAGING_DIR.to_string()),
            staging_cap_mib: 10 * 1024,
        }
    }
}

/// W2 / fix-113 (native-tar-no-quiesce, 2026-09-27): the retention a stack's
/// repositories are kept by — the stack file's own policy when it states one,
/// else the fleet-wide tiers. `backup` resolved this for compose stacks only;
/// native units always got the fleet-wide tiers.
pub fn stack_tiers(
    state: &crate::state::HostState,
    stack: &str,
    fleet: &[crate::retention::RetentionTier],
) -> Vec<crate::retention::RetentionTier> {
    state
        .stacks
        .get(stack)
        .and_then(|s| s.manifest.as_ref())
        .and_then(|m| m.retention.clone())
        .unwrap_or_else(|| fleet.to_vec())
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

/// feat-overview-10: every snapshot time across a stack's per-app
/// repositories, unix seconds, one entry per snapshot (not deduplicated by
/// night — the calendar groups them itself). A repository that does not
/// exist yet, or that fails to answer, is left out rather than failing the
/// whole stack: the calendar then simply shows nothing for that night,
/// which is the same fail-safe direction as [`newest_snapshot_unix`].
pub async fn snapshot_nights_unix(
    exec: &dyn Executor,
    m: &StackManifest,
    cfg: &BackupCfg,
) -> Vec<u64> {
    let mut out = Vec::new();
    for (owner, _paths) in owner_groups(m) {
        let Ok(res) = exec
            .run(&restic_cmd(cfg, &owner, &["snapshots", "--json"], 120))
            .await
        else {
            continue;
        };
        if !res.success() {
            continue;
        }
        out.extend(
            parse_snapshots_json(&res.stdout)
                .into_iter()
                .map(|(_, t)| t),
        );
    }
    out
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

/// T82 (F292): who keeps a repository two stacks both claim.
///
/// The F285 guard was symmetric: it saw two stacks naming one owner and
/// refused both, so a throwaway drill stack that borrowed the name
/// `jobtracker` switched off the REAL JobTracker's backup for six minutes.
/// Ownership has an order: the stack that was recorded first keeps the
/// repository, and only the newcomer is refused. A stack with no state
/// entry at all is by definition the newcomer. Equal timestamps favour the
/// caller — that is one deploy, not two stacks.
pub fn repository_keeper(this_applied_at: Option<u64>, other_applied_at: Option<u64>) -> bool {
    match (this_applied_at, other_applied_at) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(a), Some(b)) => a <= b,
    }
}

/// M-T75: the nightly backup phase says how long it took, in minutes and
/// seconds (hours past sixty minutes), so the concurrency setting can be
/// judged against a measurement instead of a prediction. One at a time
/// measured 38 min on 2026-09-02; three at a time was predicted at about
/// 13 min and never read.
pub fn phase_duration_line(secs: u64, stacks: usize, at_a_time: usize) -> String {
    let human = if secs >= 3600 {
        format!(
            "{} h {} min {} s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    } else if secs >= 60 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{} s", secs)
    };
    format!(
        "scheduler: backup phase took {} for {} stack(s), {} at a time",
        human, stacks, at_a_time
    )
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
    let mut newcomer_named: Option<(String, String)> = None;
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
            // T82: the incumbent keeps the repository; the newcomer is refused.
            let this_at = snapshot.stacks.get(&m.stack_name).map(|s| s.applied_at);
            let other_at = snapshot.stacks.get(&other).map(|s| s.applied_at);
            if repository_keeper(this_at, other_at) {
                newcomer_named = Some((owner.clone(), other.clone()));
                return Ok(StepOutcome::Unchanged);
            }
            return Err(CoreError::SafetyAbort(format!(
                "stack '{}' would back up into the repository '{}-config', which stack '{}' \
                 already owns :: repositories are named after the owning app (D25), so both \
                 stacks write into ONE history and the retention pass afterwards applies this \
                 stack's tiers to the other stack's snapshots. The stack recorded first keeps \
                 the repository; rename the app in this stack file",
                m.stack_name, owner, other
            )));
        }
        Ok(StepOutcome::Unchanged)
    });
    if let Some((owner, other)) = &newcomer_named {
        runner.log(
            Level::Warn,
            format!(
                "[owner] stack '{}' also claims '{}' but was recorded later — it is the \
                 newcomer and ITS backup is refused, not this one (T82)",
                other, owner
            ),
        );
    }

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
                crate::ops::busy::app_busy(ctx.exec, &ctx.state_dir, m.vmid, &m.stack_name, app).await?
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
                            "find {} -mindepth 1 -maxdepth 1 2>/dev/null | head -5 | wc -l",
                            shq(&mount.host_path)
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
                        &format!("test -d {} && echo yes || echo no", shq(&mount.host_path)),
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
        // gap-24: the answer is read. An existing repository says so and is
        // the normal case; a new one is checked against host-meta's password.
        let mut created = false;
        for (owner, _) in &groups {
            let init = restic(&cfg.restic_base, owner, &cfg.password_file, &["init"], 120);
            if init_repository(exec, &init, owner).await? == InitOutcome::Created {
                same_password_as_host_meta(exec, &cfg.restic_base, &cfg.password_file, owner)
                    .await?;
                created = true;
            }
        }
        Ok(if created {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
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

    let run_tag = run_tag(ctx.now_unix);

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
                // fix-112: every repository of one stack's night carries the
                // same tag, so a restore can take one night across all of
                // them instead of each repository's own newest.
                let mut args = vec!["backup", "--quiet", "--json", "--tag", &run_tag];
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
            .map(|a| {
                super::util::app_dir_script(&m.stack_name, a)
                    .raw("docker compose up -d")
                    .build()
            })
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

/// fix-112: the tag one night's backups carry, the same in every repository
/// of the stack.
pub fn run_tag(now_unix: u64) -> String {
    format!("run-{}", now_unix)
}

/// fix-112: one snapshot as a restore sees it — its ids, its time, and the
/// night it belongs to when it carries a `run-<unix>` tag.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapRun {
    pub id: String,
    pub short_id: String,
    pub time: u64,
    pub run: Option<u64>,
}

/// fix-112: `restic snapshots --json`, with the night tag. Malformed input
/// gives an empty list, which the resolution below refuses.
pub fn parse_snapshot_runs(raw: &str) -> Vec<SnapRun> {
    #[derive(serde::Deserialize)]
    struct Snap {
        id: String,
        short_id: String,
        time: String,
        #[serde(default)]
        tags: Option<Vec<String>>,
    }
    let Ok(snaps) = serde_json::from_str::<Vec<Snap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps
        .into_iter()
        .filter_map(|s| {
            let time = humantime_to_unix(&s.time)?;
            let run = s
                .tags
                .unwrap_or_default()
                .iter()
                .find_map(|t| t.strip_prefix("run-")?.parse::<u64>().ok());
            Some(SnapRun {
                id: s.id,
                short_id: s.short_id,
                time,
                run,
            })
        })
        .collect()
}

/// fix-112: `(repository, snapshot id)` per repository, and a note to log
/// when the choice could not keep to one night.
pub type NightChoice = (Vec<(String, String)>, Option<String>);

/// fix-112: the snapshot of each repository that a restore of `wanted` takes.
///
/// `latest` is the newest night present in EVERY repository. When no night
/// is (history from before the tags), each repository's own newest is taken
/// and the note says so. An explicit id brings the other repositories of its
/// night along, and is refused when one of them does not have that night.
pub fn resolve_night(
    listings: &[(String, Vec<SnapRun>)],
    wanted: &str,
) -> Result<NightChoice, String> {
    for (owner, snaps) in listings {
        if snaps.is_empty() {
            return Err(format!(
                "repository for '{}' holds no snapshots at all — nothing has been stopped",
                owner
            ));
        }
    }
    let of_night = |run: u64| -> Result<Vec<(String, String)>, String> {
        listings
            .iter()
            .map(|(owner, snaps)| {
                snaps
                    .iter()
                    .filter(|s| s.run == Some(run))
                    .max_by_key(|s| s.time)
                    .map(|s| (owner.clone(), s.id.clone()))
                    .ok_or_else(|| owner.clone())
            })
            .collect()
    };
    if wanted == "latest" {
        let mut common: Option<std::collections::BTreeSet<u64>> = None;
        for (_, snaps) in listings {
            let runs: std::collections::BTreeSet<u64> =
                snaps.iter().filter_map(|s| s.run).collect();
            common = Some(match common {
                None => runs,
                Some(c) => c.intersection(&runs).cloned().collect(),
            });
        }
        if let Some(run) = common.and_then(|c| c.into_iter().max()) {
            return of_night(run).map(|ids| (ids, None));
        }
        let ids = listings
            .iter()
            .map(|(owner, snaps)| {
                let newest = snaps.iter().max_by_key(|s| s.time).expect("checked above");
                (owner.clone(), newest.id.clone())
            })
            .collect();
        return Ok((
            ids,
            Some(
                "no night is present in every repository (snapshots from before the night \
                 tags), so each repository's own newest is restored — they may be from \
                 different nights"
                    .into(),
            ),
        ));
    }
    let found = listings.iter().find_map(|(owner, snaps)| {
        snaps
            .iter()
            .find(|s| s.id.starts_with(wanted) || s.short_id.starts_with(wanted))
            .map(|s| (owner, s))
    });
    let Some((owner, snap)) = found else {
        return Err(format!(
            "snapshot '{}' is in none of this stack's repositories — nothing has been stopped",
            wanted
        ));
    };
    let Some(run) = snap.run else {
        return Err(format!(
            "snapshot '{}' of '{}' carries no night tag (it is older than fix-112), so the \
             other repositories cannot be matched to it — restore that app alone with `--app \
             {}`, or restore 'latest'. Nothing has been stopped",
            wanted, owner, owner
        ));
    };
    of_night(run).map(|ids| (ids, None)).map_err(|missing| {
        format!(
            "snapshot '{}' is from the night run-{}, which the repository for '{}' does not \
             have — restoring it would pair two nights. Nothing has been stopped",
            wanted, run, missing
        )
    })
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

/// fix-64 (restore-no-confirm-no-safety-snapshot, 2026-09-27): has the
/// operator typed the stack's name for this restore? The TUI always asked;
/// the command line went straight to the host, and the host took either. A
/// restore overwrites live data, so the host refuses one that does not carry
/// the name, whoever sent it.
pub fn restore_confirmed(stack: &str, confirm: Option<&str>) -> Result<(), CoreError> {
    match confirm {
        Some(typed) if typed == stack => Ok(()),
        Some(typed) => Err(CoreError::SafetyAbort(format!(
            "restore of '{}' refused: the typed name '{}' does not match — nothing was stopped",
            stack, typed
        ))),
        None => Err(CoreError::SafetyAbort(format!(
            "restore of '{}' refused: the request carries no typed stack name — a client from \
             before fix-64 cannot restore; update it (`homelab restore` asks for the name, \
             `--yes` answers it for scripts)",
            stack
        ))),
    }
}

/// E2 with the safety copy (fix-64) on. See `restore_with`.
pub async fn restore(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> OperationReport {
    restore_with(ctx, m, cfg, snapshot, true).await
}

/// fix-64: where a restore keeps the data it is about to overwrite.
fn pre_restore_dir(ctx: &OpCtx<'_>, m: &StackManifest) -> String {
    format!(
        "{}/pre-restore/{}-{}",
        ctx.state_dir, m.stack_name, ctx.now_unix
    )
}

/// E2, and `safety_copy` says whether the current data is copied aside
/// first (fix-64). Off only on the operator's explicit `--no-safety-copy`.
pub async fn restore_with(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
    safety_copy: bool,
) -> OperationReport {
    restore_app(ctx, m, cfg, snapshot, safety_copy, None).await
}

/// fix-112: the repositories one app's data lives in, or the whole stack's
/// when `app` is None. The same rule as the pre-update copy: a path names its
/// app, and a path without one belongs to the stack's only app.
fn restore_groups(
    m: &StackManifest,
    app: Option<&str>,
) -> Result<Vec<(String, Vec<String>)>, CoreError> {
    let all = owner_groups(m);
    let Some(a) = app else {
        return Ok(all);
    };
    if !m.apps.iter().any(|x| x == a) {
        return Err(CoreError::Validation(format!(
            "stack '{}' has no app '{}' (it has: {}) — nothing has been stopped",
            m.stack_name,
            a,
            m.apps.join(", ")
        )));
    }
    let owners: Vec<String> = m
        .storage
        .iter()
        .filter(|s| !s.no_data && s.no_backup.is_none())
        .filter(|s| match s.app.as_deref() {
            Some(x) => x == a,
            None => m.apps.len() == 1,
        })
        .map(|s| s.owner(&m.stack_name).to_string())
        .collect();
    let groups: Vec<(String, Vec<String>)> = all
        .into_iter()
        .filter(|(o, _)| owners.contains(o))
        .collect();
    if groups.is_empty() {
        return Err(CoreError::Validation(format!(
            "app '{}' of stack '{}' keeps no backed-up data, so there is nothing to restore — \
             nothing has been stopped",
            a, m.stack_name
        )));
    }
    Ok(groups)
}

/// E2 for one app of a stack (`app`), or the whole stack (None).
///
/// fix-112: three changes (story: `docs/deployment/REGISTER.md`). Only the
/// named app is stopped and restored, so rolling back one broken Sonarr no
/// longer rolls back Radarr and Jellyfin. A stack with several repositories
/// restores one NIGHT across all of them (the `run-<unix>` tag every backup
/// writes), never each repository's own newest. And with the safety copy
/// taken, each target is emptied before restic writes into it, because a
/// restore over a non-empty directory leaves every file the snapshot does
/// not have (newer Postgres WAL segments beside a restored cluster is a
/// corrupt database that "restored successfully").
pub async fn restore_app(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
    safety_copy: bool,
    app: Option<&str>,
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

    // gap-28: a native unit's backup is one tar stream, `/<unit>-data.tar`
    // (native.rs), not a copy of its directories. `restic restore --target /`
    // of that repository drops the tar file at `/` on the Proxmox host,
    // unpacks nothing, and used to report success. Refused before anything
    // is touched, naming the procedure that does work.
    step!(runner, "native units", {
        let natives: Vec<String> = owner_groups(m)
            .into_iter()
            .map(|(owner, _)| owner)
            .filter(|o| m.natives.contains(o))
            .collect();
        if !natives.is_empty() {
            return Err(CoreError::SafetyAbort(format!(
                "{} is a native unit: its backup is one tar stream, and a restic restore would \
                 drop that tar file on the host and unpack nothing :: restore it by hand with \
                 `restic dump` into the container, as docs/OPERATIONS_RUNBOOK.md op-11 describes",
                natives.join(", ")
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // D25: a stack's data lives in one repository per owning app, so a
    // restore walks all of them. Order is the manifest's. fix-112: only the
    // named app's, when one is named.
    let groups = match restore_groups(m, app) {
        Ok(g) => g,
        Err(e) => return runner.finish_err("select app", &e),
    };
    let apps: Vec<String> = match app {
        Some(a) => vec![a.to_string()],
        None => m.apps.clone(),
    };

    // G5 of the Phase-7 gate: this step was called "validate snapshot" and
    // never looked at the snapshot. It took the caller's id, asked each
    // repository whether it answered at all, and returned — so a typo'd id
    // passed here, the stack was composed down, and restic then failed on
    // something that does not exist. The name promised the check; only the
    // name.
    // fix-112: which snapshot of each repository this restore takes.
    let mut chosen: Vec<(String, String)> = groups
        .iter()
        .map(|(o, _)| (o.clone(), snapshot.to_string()))
        .collect();
    let mut night_note: Option<String> = None;
    step!(runner, "validate snapshot", {
        if groups.len() > 1 {
            let mut listings: Vec<(String, Vec<SnapRun>)> = Vec::new();
            for (owner, _) in &groups {
                let out = exec
                    .run(&restic(
                        &cfg.restic_base,
                        owner,
                        &cfg.password_file,
                        &["snapshots", "--json"],
                        120,
                    ))
                    .await?;
                if !out.success() {
                    return Err(CoreError::Other(format!(
                        "restic repo for '{}' unreachable",
                        owner
                    )));
                }
                listings.push((owner.clone(), parse_snapshot_runs(&out.stdout)));
            }
            let (ids, note) = resolve_night(&listings, snapshot).map_err(CoreError::Other)?;
            chosen = ids;
            night_note = note;
            return Ok(StepOutcome::Unchanged);
        }
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

    if let Some(note) = &night_note {
        runner.log(Level::Warn, format!("[restore] {}", note));
    }
    if chosen.len() > 1 {
        runner.log(
            Level::Info,
            format!(
                "[restore] one night across {} repositories: {}",
                chosen.len(),
                chosen
                    .iter()
                    .map(|(o, id)| format!("{} {}", o, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }

    // fix-64: when the damage happened before last night's backup, `latest`
    // IS the damage, and the restore used to overwrite the only other copy
    // of the data. The current data is copied aside first, with the stack
    // down, into a directory of its own that nothing ever removes. Whether
    // there is room is asked before anything is stopped.
    let copy_dest = pre_restore_dir(ctx, m);
    let all_paths: Vec<String> = groups
        .iter()
        .flat_map(|(_, paths)| paths.iter().cloned())
        .collect();
    if safety_copy {
        step!(runner, "room for a safety copy", {
            let quoted: Vec<String> = all_paths.iter().map(|p| super::util::shq(p)).collect();
            let sizes = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "du -sbc {} | tail -1 | cut -f1; df -B1 --output=avail {} | tail -1; \
                             test -e {} && echo exists || true",
                            quoted.join(" "),
                            super::util::shq(&ctx.state_dir),
                            super::util::shq(&copy_dest)
                        ),
                    ],
                    300,
                ))
                .await?;
            if sizes.stdout.contains("exists") {
                return Err(CoreError::SafetyAbort(format!(
                    "{} already exists, and a safety copy is never written over — nothing has \
                     been stopped",
                    copy_dest
                )));
            }
            let nums: Vec<u64> = sizes
                .stdout
                .split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect();
            let enough = match nums[..] {
                [size, avail] => avail >= size.saturating_mul(2).saturating_add(1 << 30),
                _ => false,
            };
            if !enough {
                return Err(CoreError::SafetyAbort(format!(
                    "no room for a copy of the current data under {} ({}) :: the restore \
                     overwrites it, and a copy needs twice its size plus 1 GiB free. Nothing has \
                     been stopped. Free space, or restore without the copy with \
                     `--no-safety-copy`",
                    ctx.state_dir,
                    sizes.stdout.trim().replace('\n', " / ")
                )));
            }
            Ok(StepOutcome::Unchanged)
        });
    } else {
        runner.log(
            Level::Warn,
            format!(
                "[restore] --no-safety-copy: the current data of {} is overwritten without a \
                 copy, and restic writes over it in place — files the snapshot does not have \
                 stay behind (fix-112)",
                m.stack_name
            ),
        );
    }

    // Stop the whole stack for a consistent restore.
    step!(runner, "quiesce stack", {
        for a in &apps {
            let _ = super::util::compose_in_app(
                exec,
                m.vmid,
                &m.stack_name,
                a,
                "docker compose down",
                120,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    // fix-64: the copy, with the stack down so it is consistent. A copy that
    // fails stops the restore; the stack is resumed below either way.
    let copy_result = if safety_copy {
        runner
            .step("safety copy", || async {
                let quoted: Vec<String> = all_paths.iter().map(|p| super::util::shq(p)).collect();
                run_ok(
                    exec,
                    &Cmd::new(
                        "sh",
                        &[
                            "-c",
                            &format!(
                                "mkdir -p {d} && cp -a --parents {src} {d}/",
                                d = super::util::shq(&copy_dest),
                                src = quoted.join(" ")
                            ),
                        ],
                        cfg.restore_timeout_s,
                    ),
                )
                .await?;
                Ok(StepOutcome::Changed)
            })
            .await
            .map(|_| true)
    } else {
        Ok(false)
    };
    if let Ok(true) = copy_result {
        runner.log(
            Level::Warn,
            format!(
                "[restore] the data as it was before this restore is kept at {} — nothing \
                 removes it; delete it by hand once the restore is proven",
                copy_dest
            ),
        );
    }

    // G4 of the Phase-7 gate, and the same lesson `backup()` above already
    // carries in capitals: the restore may fail, but RESUME MUST ALWAYS RUN.
    // A fail-closed abort here leaves the stack composed down — after a
    // four-hour timeout on Google Drive, or a dropped connection — until a
    // human notices. That is a self-inflicted outage on the one operation
    // you run when something is already wrong.
    let restore_result = match &copy_result {
        Err(_) => Ok(StepOutcome::Unchanged),
        Ok(copied) => {
            let copied = *copied;
            runner
                .step("restore data", || async {
                    for (owner, paths) in &groups {
                        // fix-112: the data is in the safety copy, so the
                        // target can be emptied and restic writes into a
                        // clean directory. Without the copy nothing is
                        // deleted: that would be the only copy there is.
                        if copied {
                            for p in paths {
                                run_ok(
                                    exec,
                                    &Cmd::new(
                                        "sh",
                                        &[
                                            "-c",
                                            &format!(
                                                "find {} -mindepth 1 -delete",
                                                super::util::shq(p)
                                            ),
                                        ],
                                        cfg.restore_timeout_s,
                                    ),
                                )
                                .await?;
                            }
                        }
                        let id = chosen
                            .iter()
                            .find(|(o, _)| o == owner)
                            .map(|(_, id)| id.as_str())
                            .unwrap_or(snapshot);
                        run_ok(
                            exec,
                            &restic(
                                &cfg.restic_base,
                                owner,
                                &cfg.password_file,
                                &["restore", id, "--target", "/"],
                                cfg.restore_timeout_s,
                            ),
                        )
                        .await?;
                    }
                    Ok(StepOutcome::Changed)
                })
                .await
        }
    };

    step!(runner, "resume stack", {
        for a in &apps {
            super::util::compose_in_app(
                exec,
                m.vmid,
                &m.stack_name,
                a,
                "docker compose up -d",
                300,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    if let Err(e) = copy_result {
        return runner.finish_err("safety copy", &e);
    }
    if let Err(e) = restore_result {
        return runner.finish_err("restore data", &e);
    }

    step!(runner, "verify health", {
        for a in &apps {
            // fix-132/fix-133: read via `--format json` and the Unknown
            // state, so a probe that could not answer at all is reported as
            // what it is rather than folded into "nothing is running".
            match super::util::compose_running_services(exec, m.vmid, &m.stack_name, a, "").await? {
                Some(running) if running.is_empty() => {
                    return Err(CoreError::Other(format!("{} not running after restore", a)));
                }
                Some(_) => {}
                None => {
                    return Err(CoreError::Other(format!(
                        "{} :: could not read whether it is running after restore",
                        a
                    )));
                }
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    // fix-147: a restore is a check of these directories that succeeded; a
    // deploy's earlier failed check of them stops standing.
    let restored: Vec<String> = groups
        .iter()
        .flat_map(|(_, paths)| paths.iter().cloned())
        .collect();
    crate::ops::deploy::record_restore_checks(ctx, &m.stack_name, &[], &restored).await;

    runner.log(
        Level::Info,
        format!("[restore] {} restored and verified", m.stack_name),
    );
    runner.finish_ok()
}

/// Host files outside `state_dir` that the host-meta snapshot carries when
/// they exist. Public so the disaster-recovery runbook lists the same set.
pub const HOST_META_EXTRAS: &[&str] = &[
    "/usr/local/bin/smart-textfile-collector.py",
    "/etc/systemd/system/smart-collector.service",
    "/etc/systemd/system/smart-collector.timer",
    // fix-111 (host-meta-gaps, 2026-09-27): what a rebuilt host needs and no
    // backup held. The VLAN bridges every container sits on; the storage and
    // job definitions; the VM configurations, which carry Home Assistant's
    // Zigbee USB passthrough (VM 101) — read from pmxcfs on the host, the VMs
    // themselves are not touched; the swappiness drop-in; and rclone's
    // remote, without which restic cannot reach Google Drive at all. All
    // tiny, and all skipped when absent like the three above.
    "/etc/network/interfaces",
    "/etc/pve/storage.cfg",
    "/etc/pve/jobs.cfg",
    "/etc/pve/qemu-server",
    "/etc/sysctl.d/99-homelab-swappiness.conf",
    "/root/.config/rclone/rclone.conf",
];

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
    let host_extras: Vec<String> = HOST_META_EXTRAS.iter().map(|s| s.to_string()).collect();

    step!(runner, "init repo", {
        // gap-24: read the answer; only "already exists" is harmless.
        let init = restic(
            &cfg.restic_base,
            "host-meta",
            &cfg.password_file,
            &["init"],
            120,
        );
        Ok(match init_repository(exec, &init, "host-meta").await? {
            InitOutcome::Created => StepOutcome::Changed,
            InitOutcome::Existed => StepOutcome::Unchanged,
        })
    });

    // Which of the extras this host actually has. Read once, here, so the
    // snapshot step gets a list that cannot fail on a missing path.
    let mut present: Vec<String> = Vec::new();
    step!(runner, "host extras", {
        for p in &host_extras {
            let out = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", &format!("test -e {} && echo yes || true", shq(p))],
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

    // fix-111 (host-meta-gaps, 2026-09-27): this repository was never
    // pruned, so every rotated secret was kept for ever. The fleet-wide
    // tiers apply, whose last tier is unbounded: history stays, one
    // snapshot per bucket.
    step!(runner, "retention", {
        let out = run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &["snapshots", "--json"],
                300,
            ),
        )
        .await?;
        let doomed = crate::retention::forget_list(
            &parse_snapshots_json(&out.stdout),
            &cfg.tiers,
            ctx.now_unix,
        );
        if doomed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &args,
                900,
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

// ── feat-backup-1/2/3: status, snapshot list, snapshot browse ─────────────

/// feat-backup-1: one repository's status on the Backups page — D25's
/// owning-app repository, its newest snapshot, how many it holds, its size
/// on the remote, and the last restore-drill verdict recorded for it
/// (`state::DrillRecord`, keyed the same way the drill itself keys it: by
/// repository name).
// fix-64: Deserialize added so the CLI (`homelab snapshots`) can parse the
// host's GetBackups reply; the dashboard, which only ever serialized this
// to JSON for its own JS, did not need it before.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoStatus {
    /// D25: the owning app (compose) or unit (native) — the restic
    /// repository is named `<base>/<owner>-config`.
    pub owner: String,
    /// None when the repository does not exist yet (a stack never backed
    /// up) or could not be read — `error` then says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_snapshot: Option<SnapRun>,
    pub snapshot_count: usize,
    /// feat-backup-2: every snapshot of this repository, newest first — the
    /// restore dialog's own picker reads this (one round trip, no separate
    /// "list snapshots" call); the Backups page itself only ever shows
    /// `newest_snapshot`.
    pub snapshots: Vec<SnapRun>,
    /// `restic stats latest --json`'s `total_size`; None when that call
    /// failed (the status as a whole is not failed for it — a size is a
    /// nicety, not a safety fact).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<crate::state::DrillRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// feat-backup-1: `restic snapshots --json` for one repository, parsed and
/// newest-first — the data both the status card and the restore snapshot
/// picker read.
pub async fn list_snapshots(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
) -> Result<Vec<SnapRun>, CoreError> {
    let out = exec
        .run(&restic_cmd(cfg, owner, &["snapshots", "--json"], 120))
        .await?;
    // exit 10: repository does not exist (never backed up) — an empty list,
    // not an error; anything else that fails IS an error, same rule
    // `restore_empty_unit` already uses for this exact distinction.
    if !out.success() {
        if out.code == 10 {
            return Ok(Vec::new());
        }
        return Err(CoreError::Other(format!(
            "restic snapshots for '{}' failed (exit {}): {}",
            owner,
            out.code,
            out.stderr.trim()
        )));
    }
    let mut snaps = parse_snapshot_runs(&out.stdout);
    snaps.sort_by_key(|s| std::cmp::Reverse(s.time));
    Ok(snaps)
}

/// feat-backup-1: one repository's size on the remote, from `restic stats`.
/// `None` on any failure — a status page shows "unknown" rather than
/// refusing to show the rest of the row.
async fn repo_size_bytes(exec: &dyn Executor, cfg: &BackupCfg, owner: &str) -> Option<u64> {
    let out = exec
        .run(&restic_cmd(cfg, owner, &["stats", "latest", "--json"], 120))
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(out.stdout.trim())
        .ok()?
        .get("total_size")
        .and_then(|n| n.as_u64())
}

/// feat-backup-1: every repository a stack's manifest declares (D25's
/// `owner_groups`), with its status. One repository's failure to answer
/// does not hide the others — their `error` field says so individually.
pub async fn backup_status(
    exec: &dyn Executor,
    m: &StackManifest,
    cfg: &BackupCfg,
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
) -> Vec<RepoStatus> {
    let mut out = Vec::new();
    for (owner, _paths) in owner_groups(m) {
        out.push(repo_status_of(exec, cfg, drills, owner).await);
    }
    out
}

/// feat-backup-1: the same, for one already-known repository name (a native
/// unit, whose "manifest" is `NativeServiceManifest`, not `StackManifest`,
/// so it cannot go through `owner_groups`).
pub async fn repo_status_of(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
    owner: String,
) -> RepoStatus {
    match list_snapshots(exec, cfg, &owner).await {
        Ok(snaps) => RepoStatus {
            newest_snapshot: snaps.first().cloned(),
            snapshot_count: snaps.len(),
            size_bytes: repo_size_bytes(exec, cfg, &owner).await,
            drill: drills.get(&owner).cloned(),
            error: None,
            owner,
            snapshots: snaps,
        },
        Err(e) => RepoStatus {
            newest_snapshot: None,
            snapshot_count: 0,
            size_bytes: None,
            drill: drills.get(&owner).cloned(),
            error: Some(e.to_string()),
            owner,
            snapshots: Vec::new(),
        },
    }
}

/// feat-backup-3: one entry `restic ls --json` reports — a file or a
/// directory under the browsed path, read-only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEntry {
    pub name: String,
    pub path: String,
    /// "file" or "dir", restic's own `struct_type`/`type` vocabulary.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<String>,
}

/// feat-backup-3: parse `restic ls <id> --json [path]`'s newline-delimited
/// output into the entries directly under `path` (not every descendant —
/// restic `ls` without `--recursive` already stops there, this only drops
/// the leading `snapshot` summary line `ls` also emits).
pub fn parse_snapshot_ls(raw: &str) -> Vec<SnapshotEntry> {
    #[derive(serde::Deserialize)]
    struct Node {
        #[serde(default)]
        struct_type: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        path: String,
        #[serde(default, rename = "type")]
        node_type: String,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        mtime: Option<String>,
    }
    raw.lines()
        .filter_map(|l| serde_json::from_str::<Node>(l.trim()).ok())
        .filter(|n| n.struct_type == "node")
        .map(|n| SnapshotEntry {
            name: n.name,
            path: n.path,
            kind: n.node_type,
            size: n.size,
            mtime: n.mtime,
        })
        .collect()
}

/// feat-backup-3: list one snapshot's files under `path` ("" = root),
/// read-only — never writes, never stops a container.
pub async fn browse_snapshot(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
    snapshot: &str,
    path: &str,
) -> Result<Vec<SnapshotEntry>, CoreError> {
    let target = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let out = exec
        .run(&restic_cmd(
            cfg,
            owner,
            &["ls", "--json", snapshot, &target],
            120,
        ))
        .await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "restic ls {} {} for '{}' failed (exit {}): {}",
            snapshot,
            target,
            owner,
            out.code,
            out.stderr.trim()
        )));
    }
    Ok(parse_snapshot_ls(&out.stdout))
}

#[cfg(test)]
mod backup_status_tests {
    use super::*;

    #[test]
    fn empty_summary_is_empty_entries() {
        assert!(parse_snapshot_ls("").is_empty());
    }

    #[test]
    fn parses_node_lines_and_skips_the_snapshot_summary_line() {
        let raw = "{\"struct_type\":\"snapshot\",\"id\":\"abc\"}\n\
                    {\"struct_type\":\"node\",\"name\":\"a.txt\",\"path\":\"/a.txt\",\"type\":\"file\",\"size\":12}\n\
                    {\"struct_type\":\"node\",\"name\":\"sub\",\"path\":\"/sub\",\"type\":\"dir\"}\n";
        let entries = parse_snapshot_ls(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].kind, "file");
        assert_eq!(entries[0].size, Some(12));
        assert_eq!(entries[1].kind, "dir");
        assert_eq!(entries[1].size, None);
    }

    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let raw = "not json\n{\"struct_type\":\"node\",\"name\":\"ok\",\"path\":\"/ok\",\"type\":\"file\"}\n";
        let entries = parse_snapshot_ls(raw);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "ok");
    }
}
