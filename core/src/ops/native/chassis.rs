//! fix-113: the chassis-rs `backup-pause` protocol — its limits, how its
//! exit code is read, and the heartbeat that renews the pause while work
//! runs under it.

use crate::error::CoreError;
use crate::executor::{Cmd, CmdOutput, Executor, run_ok};
use crate::ops::util::shq;
use crate::ops::util_pct_sh;

/// chassis-rs `backup-pause --for <secs>`: the dead-man switch's own range
/// (the kit refuses outside 1..21600).
pub const MAX_CHASSIS_PAUSE_FOR_S: u64 = 21600;

/// fix-113 ADDENDUM (owner + chassis-rs agreement, 2026-10-01): "no guessed
/// N" — every `backup-pause --for` call, first and every renewal, asks for
/// this short a window. A run that dies mid-copy leaves the service
/// un-paused again within this long, never parked for hours on a guess.
pub const CHASSIS_PAUSE_FOR_S: u64 = 120;

/// How often the pause is renewed while work (the local copy, or — with no
/// staging — the tar-to-restic pipe itself) is still running under it.
pub const CHASSIS_HEARTBEAT_INTERVAL_S: u64 = 60;

/// fix-113 (owner decision, 2026-10-01): what `backup_native` does after
/// running `<binary> backup-pause --for <secs>` in `BackupPause::Chassis`
/// mode, decided from its exit code (chassis-rs commit 722249b) and, only
/// for exit 3, whether the unit is active — a pure function so each branch
/// is a test, not a live run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChassisPauseOutcome {
    /// Exit 0: the binary itself quiesced the service (a listener answered,
    /// or it stopped the unit, or the unit was not running) — its stdout
    /// first word says which. Resume afterwards with `backup-resume`.
    Paused(String),
    /// Exit 2 (a binary built before `backup-pause` existed: clap's own
    /// "unrecognized subcommand") or exit 3 with the unit active (no
    /// listener and the binary could not stop it either): fall back to the
    /// `BackupPause::Unit` mechanism — the homelab stops the unit itself.
    FallBackToStop,
    /// Exit 3 with the unit not active: nothing is running, so there is
    /// nothing to pause and nothing to resume.
    NothingRunning,
    /// Exit 1, or any code outside 0..=3: fail loudly — the unit started
    /// again on its own (exit 1) or the binary said something this build
    /// does not understand. Nothing was archived.
    Failed(String),
}

/// See [`ChassisPauseOutcome`]. `unit_active` is read only for exit 3 — the
/// one code whose meaning depends on it — so a caller need not probe the
/// unit for the other three.
pub fn decide_chassis_pause(
    exit_code: i32,
    stdout: &str,
    unit_active: bool,
) -> ChassisPauseOutcome {
    let stdout = stdout.trim();
    match exit_code {
        0 => ChassisPauseOutcome::Paused(stdout.to_string()),
        2 => ChassisPauseOutcome::FallBackToStop,
        3 => {
            if unit_active {
                ChassisPauseOutcome::FallBackToStop
            } else {
                ChassisPauseOutcome::NothingRunning
            }
        }
        1 => ChassisPauseOutcome::Failed(format!(
            "backup-pause failed (exit 1) — the unit started again on its own: {}",
            stdout
        )),
        other => ChassisPauseOutcome::Failed(format!(
            "backup-pause exited {} (expected 0, 1, 2 or 3): {}",
            other, stdout
        )),
    }
}

/// fix-113 ADDENDUM: renews the chassis pause every
/// [`CHASSIS_HEARTBEAT_INTERVAL_S`] while the work it is racing against
/// ([`run_under_chassis_heartbeat`]) is still running. Bounded by
/// `max_renewals` so it can only ever LOSE that race, never win it: once
/// exhausted it parks on a future that never completes rather than ending
/// the race early and leaving the other side cut off mid-copy.
async fn chassis_pause_heartbeat(exec: &dyn Executor, vmid: u16, binary: &str, max_renewals: u64) {
    for _ in 0..max_renewals {
        exec.sleep_ms(CHASSIS_HEARTBEAT_INTERVAL_S * 1000).await;
        let _ = util_pct_sh(
            exec,
            vmid,
            &format!("{} backup-pause --for {}", shq(binary), CHASSIS_PAUSE_FOR_S),
            30,
        )
        .await;
    }
    std::future::pending::<()>().await;
}

/// fix-113 ADDENDUM: runs `cmd` while the chassis pause is renewed
/// underneath it, so work that outlives the first `--for 120` window is not
/// cut off mid-write. `cmd`'s own `timeout_s` bounds how many renewals the
/// heartbeat is allowed (`+2` of headroom) — comfortably more than the
/// heartbeat could ever need before `cmd` itself resolves (success, failure
/// or its own timeout), so the heartbeat branch structurally cannot win the
/// race in [`futures_util::future::select`] (which polls the first future —
/// `cmd` — before the second on every poll, so a `cmd` that is already done,
/// as every scripted `MockExecutor` response is, never triggers a single
/// renewal: every existing test exercises this exact path unchanged).
pub(super) async fn run_under_chassis_heartbeat(
    exec: &dyn Executor,
    vmid: u16,
    binary: &str,
    cmd: &Cmd,
) -> Result<CmdOutput, CoreError> {
    let max_renewals = cmd.timeout_s / CHASSIS_HEARTBEAT_INTERVAL_S + 2;
    let work = Box::pin(run_ok(exec, cmd));
    let heartbeat = Box::pin(chassis_pause_heartbeat(exec, vmid, binary, max_renewals));
    match futures_util::future::select(work, heartbeat).await {
        futures_util::future::Either::Left((out, _heartbeat)) => out,
        futures_util::future::Either::Right(((), _work)) => {
            unreachable!("chassis_pause_heartbeat never completes before cmd's own timeout")
        }
    }
}
