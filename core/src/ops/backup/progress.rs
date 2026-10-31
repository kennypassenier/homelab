//! Reading restic's `--json` and progress output: bytes done, packs copied,
//! and whether a snapshot stored anything.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::sink::PipelineEvent;

/// ask-kp11be-5: how often restic prints a status line while it works
/// (`RESTIC_PROGRESS_FPS`, frames per second): one every 5 s. Restic's own
/// default for a non-terminal is one a minute, too slow for a job panel's
/// bar; more often only fills the connection with lines nobody reads.
pub(super) const RESTIC_PROGRESS_FPS: &str = "0.2";

/// ask-kp11be-5: one status or summary line of `restic backup --json` or
/// `restic restore --json`, as the bytes it reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResticBytes {
    pub done: u64,
    pub total: Option<u64>,
    /// A status line is progress only and is not kept in the command's
    /// output; the summary line is kept (the empty-snapshot check reads it).
    pub is_status: bool,
}

/// Read one line of restic's JSON output. `None` for anything that is not
/// a status or summary line carrying bytes: a line this does not recognise
/// is left exactly where it was.
///
/// * backup status: `bytes_done` of `total_bytes` (the total is absent
///   while restic is still scanning, or 0)
/// * restore status: `bytes_restored` of `total_bytes`
/// * summary: backup `total_bytes_processed`, restore `bytes_restored`;
///   the whole of it done
pub fn restic_bytes(line: &str) -> Option<ResticBytes> {
    let line = line.trim();
    if !line.starts_with('{') || !line.contains("\"message_type\"") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let num = |k: &str| v.get(k).and_then(|n| n.as_u64());
    match v.get("message_type")?.as_str()? {
        "status" => {
            let done = num("bytes_done").or_else(|| num("bytes_restored"))?;
            Some(ResticBytes {
                done,
                total: num("total_bytes").filter(|t| *t > 0),
                is_status: true,
            })
        }
        "summary" => {
            let done = num("total_bytes_processed").or_else(|| num("bytes_restored"))?;
            Some(ResticBytes {
                done,
                total: Some(done),
                is_status: false,
            })
        }
        _ => None,
    }
}

/// Run a restic command that reports bytes (`--json`), telling the sink
/// how far it is as it goes: `Bytes { op, label: <repository owner> }`.
/// Fails like [`run_ok`](crate::executor::run_ok) on a non-zero exit.
pub(super) async fn run_restic_bytes(
    exec: &dyn Executor,
    sink: &dyn crate::sink::Sink,
    op: &str,
    owner: &str,
    cmd: Cmd,
) -> Result<crate::executor::CmdOutput, CoreError> {
    let mut cmd = cmd;
    // Right after `env`, with the other variables.
    cmd.args
        .insert(0, format!("RESTIC_PROGRESS_FPS={RESTIC_PROGRESS_FPS}"));
    // backup-stall: a status line only counts as progress when its byte
    // count moved; restic keeps printing them while it waits on a dead
    // remote.
    let stall = cmd.stall.clone();
    let last_done = std::sync::atomic::AtomicU64::new(u64::MAX);
    let on_line = |line: &str| match restic_bytes(line) {
        Some(b) => {
            if let Some(s) = &stall
                && last_done.swap(b.done, std::sync::atomic::Ordering::Relaxed) != b.done
            {
                s.progressed();
            }
            sink.emit(PipelineEvent::Bytes {
                op: op.to_string(),
                label: owner.to_string(),
                done: b.done,
                total: b.total,
            });
            b.is_status
        }
        None => false,
    };
    let out = exec.run_streaming(&cmd, &on_line).await?;
    ensure_exit_zero(&cmd, &out)?;
    Ok(out)
}

/// Fails like [`run_ok`](crate::executor::run_ok) on a non-zero exit.
fn ensure_exit_zero(cmd: &Cmd, out: &crate::executor::CmdOutput) -> Result<(), CoreError> {
    if out.code != 0 {
        return Err(CoreError::Command {
            rendered: cmd.rendered(),
            detail: format!("rc={} :: {}", out.code, out.stderr.trim()),
        });
    }
    Ok(())
}

/// backup-local-first: how far a `restic copy` is, from the status line it
/// prints at `RESTIC_PROGRESS_FPS` when no terminal is attached
/// (`[0:42] 37.50%  3 / 8 packs copied`): the packs done.
pub fn copy_packs_done(line: &str) -> Option<u64> {
    let rest = line.trim().strip_suffix("packs copied")?.trim_end();
    let (done, _total) = rest.rsplit_once('/')?;
    done.split_whitespace().last()?.parse().ok()
}

/// backup-local-first: copy every snapshot `local` holds and `remote`
/// lacks into `remote` (restic skips what is already there), under a stall
/// watch. Progress lines are consumed, so the kept output is restic's
/// per-snapshot lines.
pub(crate) async fn run_restic_copy(exec: &dyn Executor, cmd: Cmd) -> Result<(), CoreError> {
    let mut cmd = cmd;
    cmd.args
        .insert(0, format!("RESTIC_PROGRESS_FPS={RESTIC_PROGRESS_FPS}"));
    let stall = cmd.stall.clone();
    let last = std::sync::atomic::AtomicU64::new(u64::MAX);
    let on_line = |line: &str| {
        let progressed = match copy_packs_done(line) {
            Some(n) => last.swap(n, std::sync::atomic::Ordering::Relaxed) != n,
            // A finished snapshot ("snapshot … saved, copied from …") is
            // progress too.
            None => line.contains("copied from"),
        };
        if progressed && let Some(s) = &stall {
            s.progressed();
        }
        copy_packs_done(line).is_some()
    };
    let out = exec.run_streaming(&cmd, &on_line).await?;
    ensure_exit_zero(&cmd, &out)
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
