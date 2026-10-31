//! Native backup, the pause: quiesce the service so its store is not
//! archived mid-write. fix-113: a service declared `backup_pause` is
//! stopped (`Unit`) or asks its own binary to quiesce (`Chassis`), failing
//! loudly when it will not — a tar of a store still being written is the
//! torn backup this exists to prevent.

use super::chassis::{
    CHASSIS_PAUSE_FOR_S, ChassisPauseOutcome, decide_chassis_pause, run_under_chassis_heartbeat,
};
use super::helpers::quoted_dirs;
use super::staging::{estimate_data_bytes, fits_staging, remove_staging_file, staging_free_bytes};
use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::native::NativeServiceManifest;
use crate::ops::backup::BackupCfg;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// What the pause decided, for the snapshot and resume that follow.
#[derive(Default)]
pub(super) struct PauseState {
    /// Whether the resume phase has anything to do: `Unit` always does (it
    /// always stopped the unit); `Chassis` does whenever the pause phase
    /// actually quiesced or stopped something (`Paused` or the stop
    /// fallback) — not when nothing was running to begin with, which is the
    /// one case with nothing to put back.
    pub needs_resume: bool,
    /// `Chassis` / `Paused` only: the binary itself is what quiesced things,
    /// so it is also what un-quiesces them, ahead of the homelab's own
    /// "is it active" check. The stop fallback is symmetrical with `Unit`
    /// instead — the homelab did its own `systemctl stop`, so its own
    /// `systemctl start` undoes it.
    pub chassis_resume: bool,
    /// What the pause step logs once it is done (the step closures may not
    /// log themselves).
    pub pause_note: Option<String>,
    /// fix-113 ADDENDUM: where the LOCAL staged copy landed, when staging was
    /// used. Its presence is what tells "snapshot" to read a file instead of
    /// the container, and the final cleanup to remove it; its absence with
    /// `chassis_resume` true is what tells "snapshot" to hold the pause open
    /// (renewed) across the tar-to-restic pipe itself.
    pub staged_file: Option<String>,
    /// fix-113: whatever the "resume the service" step logs — set there, or
    /// (ADDENDUM) right after a staged local copy, which resumes long before
    /// that step runs.
    pub resume_note: Option<String>,
}

/// Stop the unit for its backup; it must stop.
async fn stop_for_backup(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
) -> Result<(), CoreError> {
    let out = util_pct_sh(exec, m.vmid, &format!("systemctl stop {}", unit), 120).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "{} would not stop for its backup ({}) — nothing was archived",
            unit,
            out.stderr.trim()
        )));
    }
    Ok(())
}

/// "pause the service": `BackupPause::Unit` — stop it.
pub(super) async fn pause_unit(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
    state: &mut PauseState,
) -> Result<StepOutcome, CoreError> {
    stop_for_backup(exec, m, unit).await?;
    state.needs_resume = true;
    Ok(StepOutcome::Changed)
}

/// Exit 3 ("impossible") only decides anything once we also know whether
/// the unit is still active; for the other codes it is not read.
async fn unit_active_if_needed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
    exit_code: i32,
) -> Result<bool, CoreError> {
    if exit_code != 3 {
        return Ok(false);
    }
    let probe = util_pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
    Ok(probe.stdout.trim() == "active")
}

/// "pause the service (chassis)": ask the binary to quiesce itself and act
/// on its exit code ([`decide_chassis_pause`]).
pub(super) async fn pause_chassis(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    unit: &str,
    state: &mut PauseState,
) -> Result<StepOutcome, CoreError> {
    let ask = format!(
        "{} backup-pause --for {}",
        shq(&m.binary),
        CHASSIS_PAUSE_FOR_S
    );
    let out = util_pct_sh(exec, m.vmid, &ask, 60).await?;
    let unit_active = unit_active_if_needed(exec, m, unit, out.code).await?;
    match decide_chassis_pause(out.code, &out.stdout, unit_active) {
        ChassisPauseOutcome::Paused(mode) => {
            state.pause_note = Some(format!("backup-pause: {}", mode));
            paused(exec, cfg, m, unit, state).await
        }
        ChassisPauseOutcome::FallBackToStop => {
            state.pause_note = Some(format!(
                "backup-pause could not quiesce it (rc={}) — falling back to stopping the unit",
                out.code
            ));
            stop_for_backup(exec, m, unit).await?;
            state.needs_resume = true;
            Ok(StepOutcome::Changed)
        }
        ChassisPauseOutcome::NothingRunning => {
            state.pause_note = Some("backup-pause: not running, nothing to pause".into());
            Ok(StepOutcome::Unchanged)
        }
        ChassisPauseOutcome::Failed(why) => Err(CoreError::Other(format!(
            "{} backup-pause: {} — nothing was archived",
            unit, why
        ))),
    }
}

/// The service is quiesced. fix-113 ADDENDUM (owner + chassis-rs,
/// 2026-10-01): the pause is held open, renewed, only for a LOCAL copy —
/// never for restic's own upload. When a staging directory is configured and
/// the copy is estimated to fit it, tar it there under the renewed pause and
/// resume the SERVICE the moment that local copy is done (not after the
/// possibly-slow upload). Otherwise back up live under the renewed pause
/// (the "snapshot" step renews it across the whole tar-to-restic pipe).
async fn paused(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    unit: &str,
    state: &mut PauseState,
) -> Result<StepOutcome, CoreError> {
    if let Some(dir) = &cfg.staging_dir
        && let Some(stage_path) = staging_target(exec, cfg, m, dir).await
    {
        return stage_copy(exec, cfg, m, unit, &stage_path, state).await;
    }
    state.needs_resume = true;
    state.chassis_resume = true;
    Ok(StepOutcome::Changed)
}

/// Where to stage the copy, when it is estimated to fit `dir`; `None` means
/// back up live.
async fn staging_target(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    dir: &str,
) -> Option<String> {
    let mkdir = format!("mkdir -p {}", shq(dir));
    let _ = exec.run(&Cmd::new("sh", &["-c", &mkdir], 30)).await;
    let stage_path = format!("{}/{}-stage.tar", dir, m.unit);
    // rule 20: a leftover from a run that died between writing this file and
    // deleting it again, cleared at the start of THIS run before the
    // free-space reading below is trusted.
    remove_staging_file(exec, &stage_path).await;
    let estimate = estimate_data_bytes(exec, m.vmid, &m.data_dirs).await;
    let free = staging_free_bytes(exec, dir).await;
    matches!(
        (estimate, free),
        (Some(e), Some(f)) if fits_staging(e, f, cfg.staging_cap_mib)
    )
    .then_some(stage_path)
}

/// Tar the data dirs into `stage_path` under the renewed pause, then resume
/// before anything else, success or failure — a torn local copy must not
/// leave the service paused while this run decides what to do about it.
async fn stage_copy(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    unit: &str,
    stage_path: &str,
    state: &mut PauseState,
) -> Result<StepOutcome, CoreError> {
    let tar_script = format!(
        "set -o pipefail; pct exec {} -- tar -cf - {} > {}",
        m.vmid,
        quoted_dirs(m),
        shq(stage_path)
    );
    let tar_cmd = Cmd::new("sh", &["-c", &tar_script], cfg.snapshot_timeout_s);
    let tar_result = run_under_chassis_heartbeat(exec, m.vmid, &m.binary, &tar_cmd).await;
    let resume = format!("{} backup-resume", shq(&m.binary));
    let resume_out = util_pct_sh(exec, m.vmid, &resume, 60).await;
    match tar_result {
        Ok(_) => {
            state.resume_note = resume_out
                .as_ref()
                .ok()
                .map(|o| format!("backup-resume: {}", o.stdout.trim()));
            state.staged_file = Some(stage_path.to_string());
            Ok(StepOutcome::Changed)
        }
        Err(e) => {
            remove_staging_file(exec, stage_path).await;
            Err(CoreError::Other(format!(
                "{} local staging copy failed ({}) — resume: {:?}; nothing was archived",
                unit,
                e,
                resume_out.map(|o| o.stdout)
            )))
        }
    }
}
