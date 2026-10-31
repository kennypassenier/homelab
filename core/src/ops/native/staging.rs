//! fix-113 ADDENDUM: the local staging copy of a service's data dirs — the
//! size estimate, the free-space reading, the fit decision and the cleanup
//! of a leftover tar. Only the decision is pure; the readings are I/O done
//! through the executor.

use crate::executor::{Cmd, Executor};
use crate::ops::util::shq;
use crate::ops::util_pct_sh;

/// The safety margin `fits_staging` asks of a staged copy's estimated size
/// before it trusts the estimate against free space and the cap.
const STAGING_SAFETY_MARGIN_PCT: u64 = 20;

/// fix-113 ADDENDUM: whether a staged local copy of `estimated_bytes` is
/// trusted to fit `free_bytes` of disk and the `cap_mib` staging cap, both
/// after a 20% margin on the estimate. Pure — the `du`/`df` readings that
/// feed it are the only I/O, done by the caller.
pub fn fits_staging(estimated_bytes: u64, free_bytes: u64, cap_mib: u64) -> bool {
    let needed = estimated_bytes.saturating_mul(100 + STAGING_SAFETY_MARGIN_PCT) / 100;
    let cap_bytes = cap_mib.saturating_mul(1024 * 1024);
    needed <= free_bytes && needed <= cap_bytes
}

/// fix-113 ADDENDUM: `du -scb` over every data dir, inside the container —
/// `None` when it cannot be read (no `du`, or a dir does not exist), in
/// which case the caller skips staging rather than guessing a size.
pub(super) async fn estimate_data_bytes(
    exec: &dyn Executor,
    vmid: u16,
    data_dirs: &[String],
) -> Option<u64> {
    if data_dirs.is_empty() {
        return Some(0);
    }
    let dirs = data_dirs
        .iter()
        .map(|d| shq(d))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!("du -scb {} 2>/dev/null | tail -1 | cut -f1", dirs);
    let out = util_pct_sh(exec, vmid, &script, 60).await.ok()?;
    out.success()
        .then(|| out.stdout.trim().parse::<u64>().ok())?
}

/// fix-113 ADDENDUM: free bytes on the filesystem holding `dir` (created
/// first if missing) on THIS host — the staging directory is never inside
/// the container. `None` when it cannot be read.
pub(super) async fn staging_free_bytes(exec: &dyn Executor, dir: &str) -> Option<u64> {
    let script = format!(
        "mkdir -p {d} && df -B1 --output=avail {d} | tail -1",
        d = shq(dir)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 30)).await.ok()?;
    out.success()
        .then(|| out.stdout.trim().parse::<u64>().ok())?
}

/// fix-113 ADDENDUM: deletes a staged tar — called before trusting a free-
/// space reading (a leftover from a run that died between writing it and
/// deleting it again, rule 20) and after every staged snapshot, success or
/// failure, so one is never left behind for the next run to find.
pub(super) async fn remove_staging_file(exec: &dyn Executor, path: &str) {
    let _ = exec
        .run(&Cmd::new(
            "sh",
            &["-c", &format!("rm -f {}", shq(path))],
            30,
        ))
        .await;
}
