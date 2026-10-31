//! Native backup, the archive itself: the snapshot, the resume of a paused
//! service and the retention that follows.

use super::backup_pause::PauseState;
use super::chassis::run_under_chassis_heartbeat;
use super::helpers::quoted_dirs;
use crate::error::CoreError;
use crate::executor::{Cmd, Executor, run_ok};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::ops::backup::{BackupCfg, RESTIC_CACHE_DIR, restic_cmd, retention_doomed, stack_tiers};
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::retention::RetentionTier;
use crate::runner::StepOutcome;

/// The `restic backup --stdin` end of every snapshot pipe. D25: named after
/// the SERVICE, not the stack. T5 puts several services on one container,
/// and a per-stack repository would fold them into one — so moving any of
/// them elsewhere would leave its history behind, which is what D25 exists
/// to prevent. F171: the same restic cache constant `backup.rs` uses for
/// every compose stack, so this hand-built environment does not drift from
/// it. Story: docs/deployment/REGISTER.md.
fn restic_stdin_sink(cfg: &BackupCfg, m: &NativeServiceManifest) -> String {
    format!(
        "env RESTIC_REPOSITORY={}/{}-config RESTIC_PASSWORD_FILE={} RESTIC_CACHE_DIR={} \
         restic backup --stdin --stdin-filename {}-data.tar --tag {}",
        cfg.restic_base,
        m.unit,
        cfg.password_file,
        RESTIC_CACHE_DIR,
        m.unit,
        cfg.trigger.tag()
    )
}

/// "snapshot": fix-113 ADDENDUM: a staged local copy already landed on this
/// host (and the service is already resumed) — read it instead of the
/// container, so this step's own duration (restic's upload) never touches
/// the pause. Otherwise the container's data (or the service's own copy) is
/// piped straight into restic; with `chassis_resume` the pause is held open
/// and renewed across the tar AND the restic upload together.
pub(super) async fn snapshot(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    own_copy: Option<&str>,
    state: &PauseState,
) -> Result<StepOutcome, CoreError> {
    let sink = restic_stdin_sink(cfg, m);
    if let Some(stage_path) = &state.staged_file {
        let script = format!("set -o pipefail; cat {} | {}", shq(stage_path), sink);
        run_ok(
            exec,
            &Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s),
        )
        .await?;
        return Ok(StepOutcome::Changed);
    }
    let dirs = match own_copy {
        Some(f) => shq(f),
        None => quoted_dirs(m),
    };
    // pipefail is load-bearing: without it a dead `pct exec tar` still
    // yields a "successful" empty snapshot — a backup that lies.
    let script = format!(
        "set -o pipefail; pct exec {} -- tar -cf - {} | {}",
        m.vmid, dirs, sink
    );
    let cmd = Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s);
    if state.chassis_resume {
        run_under_chassis_heartbeat(exec, m.vmid, &m.binary, &cmd).await?;
    } else {
        run_ok(exec, &cmd).await?;
    }
    Ok(StepOutcome::Changed)
}

/// "resume the service": fix-113: the paused service is started again
/// whatever the snapshot did, as `backup` resumes what it quiesced: a backup
/// that leaves a service off is worse than one that fails. Sets
/// `resume_note` for a chassis resume.
pub(super) async fn resume_service(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
    state: &mut PauseState,
) -> Result<StepOutcome, CoreError> {
    if state.chassis_resume {
        let resume = format!("{} backup-resume", shq(&m.binary));
        let out = util_pct_sh(exec, m.vmid, &resume, 60).await?;
        state.resume_note = Some(format!("backup-resume: {}", out.stdout.trim()));
    }
    let out = util_pct_sh(
        exec,
        m.vmid,
        &format!(
            "systemctl start {u}; sleep 2; [ \"$(systemctl is-active {u})\" = active ]",
            u = unit
        ),
        120,
    )
    .await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "{} did not come back after its backup — the service is DOWN and needs \
             hands now",
            unit
        )));
    }
    Ok(StepOutcome::Changed)
}

/// W2 / fix-113: the stack file's own retention, as compose stacks have
/// had since W2. Read here so every caller gets it without being told.
/// A state that cannot be read leaves the fleet's tiers in force.
pub(super) async fn effective_tiers(
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Vec<RetentionTier> {
    match crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .load()
        .await
    {
        Ok(state) => stack_tiers(&state, &m.stack_name, &cfg.tiers),
        Err(_) => cfg.tiers.clone(),
    }
}

/// "retention": forget (and prune) the snapshots the tiers no longer keep.
pub(super) async fn retention(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    tiers: &[RetentionTier],
    now_unix: u64,
) -> Result<StepOutcome, CoreError> {
    let list = restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 300);
    let out = run_ok(exec, &list).await?;
    let doomed = retention_doomed(&out.stdout, tiers, now_unix);
    if doomed.is_empty() {
        return Ok(StepOutcome::Unchanged);
    }
    let mut args: Vec<&str> = vec!["forget"];
    args.extend(doomed.iter().map(|s| s.as_str()));
    args.push("--prune");
    run_ok(exec, &restic_cmd(cfg, &m.unit, &args, 900)).await?;
    Ok(StepOutcome::Changed)
}
