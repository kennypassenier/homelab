//! Reading snapshot times across a stack's repositories.

use crate::executor::Executor;
use crate::manifest::StackManifest;

use super::config::BackupCfg;
use super::owners::owner_groups;
use super::restic::restic_cmd;
use super::snapshot_parse::parse_snapshots_json;

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
