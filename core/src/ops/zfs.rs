//! E8: ZFS snapshots + replication, absorbed into the homelab (Kenny's
//! standing wish: one system, not a cron script he forgets about).
//!
//! Replaces `/root/full_zfs_backup.sh`, which silently died twice — once
//! from a crontab typo, once from design drift (it iterated over dataset
//! names that no longer existed). Same policy, but:
//!   - jobs are declared explicitly in host.toml; nothing is auto-discovered
//!   - the destructive fallback is REFUSED, not performed. The old script,
//!     when it found no common snapshot, ran `zfs destroy` over every
//!     snapshot on the target and re-sent everything. One bad night (empty
//!     or broken source) and the whole replication history is gone. Here
//!     that path stops the job and asks for a human, unless the target is
//!     genuinely empty (first-time seed).
//!   - retention reuses the tiered engine that drives restic (G8)
//!   - it runs in the nightly plan, so failures arrive over the existing
//!     webhook/incident chain instead of an email nobody reads.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// One replication job: snapshot `source` recursively, then send the
/// difference to `target`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ZfsJob {
    pub source: String,
    pub target: String,
}

pub const SNAP_PREFIX: &str = "homelab-";

/// Dataset names we refuse to touch, whatever the config says: a job may
/// never take its own target as source (or vice versa), and neither side may
/// be a parent of the other — recursive sends would eat themselves.
pub fn job_problems(job: &ZfsJob) -> Option<String> {
    let (s, t) = (job.source.trim(), job.target.trim());
    if s.is_empty() || t.is_empty() {
        return Some("source and target must both be set".into());
    }
    if s == t {
        return Some(format!("source and target are the same dataset ({})", s));
    }
    if t.starts_with(&format!("{}/", s)) {
        return Some(format!("target {} lives inside source {}", t, s));
    }
    if s.starts_with(&format!("{}/", t)) {
        return Some(format!("source {} lives inside target {}", s, t));
    }
    None
}

/// Snapshot names (bare, without the dataset part) from `zfs list` output,
/// oldest first — EVERY snapshot on that dataset, not just ours.
///
/// Both reasons are load-bearing, and the second was learned the hard way on
/// the first live run: (1) a snapshot left by the retired cron script is a
/// perfectly good incremental base, so the migration needs no re-seed;
/// (2) foreign snapshots are what make a target "not empty" — filtering them
/// out made a populated replica look like a blank slate and turned a refusal
/// into an attempted full send. We read everything; we only ever DESTROY
/// snapshots carrying our own prefix.
pub fn parse_snap_names(list_stdout: &str, dataset: &str) -> Vec<String> {
    let prefix = format!("{}@", dataset);
    list_stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter_map(|n| n.strip_prefix(&prefix))
        .map(|n| n.to_string())
        .collect()
}

/// The newest snapshot both sides have — the base for an incremental send.
/// `source`/`target` are name lists in creation order (oldest first).
pub fn common_base(source: &[String], target: &[String]) -> Option<String> {
    source
        .iter()
        .rev()
        .find(|s| target.contains(s))
        .map(|s| s.to_string())
    // Deliberately no fallback: if there is no shared point, an incremental
    // send is impossible and the caller must decide, not guess.
}

/// `homelab-20260827-1845` → unix time, so the shared retention engine can
/// rank snapshots without a date library on the host.
pub fn snap_time(name: &str, now: u64) -> u64 {
    // Format: homelab-YYYYMMDD-HHMM. Anything unparseable is treated as
    // brand new — retention then keeps it rather than deleting blindly.
    let Some(rest) = name.strip_prefix(SNAP_PREFIX) else {
        return now;
    };
    let (date, time) = match rest.split_once('-') {
        Some(p) => p,
        None => return now,
    };
    if date.len() != 8 || time.len() != 4 {
        return now;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi)) = (
        num(&date[0..4]),
        num(&date[4..6]),
        num(&date[6..8]),
        num(&time[0..2]),
        num(&time[2..4]),
    ) else {
        return now;
    };
    // Days since epoch via the civil-from-days algorithm (no chrono).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days * 86_400 + h * 3600 + mi * 60).max(0) as u64
}

fn snapshot_label(now_unix: u64) -> String {
    // Sortable, human-readable, and parseable by snap_time. Derived from the
    // injected clock — core never reads the wall clock itself (AR1).
    let days = now_unix / 86_400;
    let secs = now_unix % 86_400;
    // civil_from_days
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{}{:04}{:02}{:02}-{:02}{:02}",
        SNAP_PREFIX,
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60
    )
}

/// Run every configured job. Fails the whole operation if any job fails —
/// a half-replicated fleet must not read as success.
pub async fn replicate(
    ctx: &OpCtx<'_>,
    jobs: &[ZfsJob],
    tiers: &[crate::retention::RetentionTier],
) -> OperationReport {
    let mut runner = Runner::new("zfs-replicate", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let label = snapshot_label(ctx.now_unix);

    step!(runner, "validate jobs", {
        if jobs.is_empty() {
            return Err(CoreError::Other(
                "no zfs jobs configured (zfs_jobs in host.toml) — nothing to do".into(),
            ));
        }
        for j in jobs {
            if let Some(p) = job_problems(j) {
                return Err(CoreError::SafetyAbort(format!(
                    "zfs job {} → {}: {}",
                    j.source, j.target, p
                )));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    for job in jobs {
        let src = job.source.clone();
        let tgt = job.target.clone();
        let snap = format!("{}@{}", src, label);

        // Both datasets must exist before anything is created or sent.
        let step_name = format!("check {} → {}", src, tgt);
        step!(runner, &step_name, {
            let s = exec
                .run(&Cmd::new("zfs", &["list", "-H", "-o", "name", &src], 60))
                .await?;
            if !s.success() {
                return Err(CoreError::Other(format!(
                    "source dataset {} not found",
                    src
                )));
            }
            Ok(StepOutcome::Unchanged)
        });

        let step_name = format!("snapshot {}", src);
        step!(runner, &step_name, {
            run_ok(exec, &Cmd::new("zfs", &["snapshot", "-r", &snap], 300)).await?;
            Ok(StepOutcome::Changed)
        });

        let step_name = format!("replicate {} → {}", src, tgt);
        step!(runner, &step_name, {
            let list = |ds: &str| {
                let ds = ds.to_string();
                async move {
                    exec.run(&Cmd::new(
                        "zfs",
                        &[
                            "list", "-H", "-t", "snapshot", "-o", "name", "-s", "creation", "-r",
                            &ds,
                        ],
                        120,
                    ))
                    .await
                }
            };
            let src_snaps = parse_snap_names(&list(&src).await?.stdout, &src);
            let tgt_out = list(&tgt).await?;
            let target_exists = tgt_out.success();
            let tgt_snaps = parse_snap_names(&tgt_out.stdout, &tgt);
            // Emptiness is a property of the whole SUBTREE, not just the top
            // dataset: the retired script's retention deleted parent
            // snapshots while children kept theirs, so a populated replica
            // can present an empty-looking parent. A full send into that is
            // exactly what must never be attempted.
            let subtree_snaps = tgt_out.stdout.lines().filter(|l| l.contains('@')).count();

            match common_base(&src_snaps, &tgt_snaps) {
                Some(base) => {
                    // Incremental: only the delta crosses the wire.
                    let from = format!("{}@{}", src, base);
                    // F177: `-x mountpoint`. `zfs send -R` carries the
                    // source's properties, and `receive` applies them — so a
                    // replica arrives claiming the LIVE path its source is
                    // mounted at. Found 2026-09-02 while following the DR
                    // runbook: `HDD18TB/replica/HDD2TB/paperless-config` and
                    // the real `HDD2TB/paperless-config` both had
                    // mountpoint=/appdata/paperwork/paperless-config with
                    // canmount=on. Which one wins after a reboot is not
                    // decided anywhere. If the copy wins, paperless runs on
                    // stale data, writes into the replica, and the next
                    // replication run overwrites those writes — with nothing
                    // anywhere saying so.
                    //
                    // Excluding the property rather than forcing canmount=off
                    // leaves the replica mountable under the replica tree,
                    // where reading it is safe and deliberate.
                    let script = format!(
                        "zfs send -RI {} {} | zfs receive -F -x mountpoint {}",
                        shell_quote(&from),
                        shell_quote(&snap),
                        shell_quote(&tgt)
                    );
                    run_ok(exec, &Cmd::new("sh", &["-c", &script], 6 * 3600)).await?;
                    Ok(StepOutcome::Changed)
                }
                None if !target_exists || subtree_snaps == 0 => {
                    // First-time seed: nothing on the target to lose.
                    // F177, same reason as the incremental branch above.
                    let script = format!(
                        "zfs send -R {} | zfs receive -F -x mountpoint {}",
                        shell_quote(&snap),
                        shell_quote(&tgt)
                    );
                    run_ok(exec, &Cmd::new("sh", &["-c", &script], 12 * 3600)).await?;
                    Ok(StepOutcome::Changed)
                }
                None => {
                    // The dangerous case the old script powered through.
                    Err(CoreError::SafetyAbort(format!(
                        "{} and {} share no snapshot, but {} already holds {} snapshot(s) \
                         (subtree included). Re-seeding would destroy that history, so this \
                         job stops here. Decide deliberately: investigate why the chain broke, \
                         or wipe the target yourself with `zfs destroy -r {}` and re-run for a \
                         fresh seed.",
                        src, tgt, tgt, subtree_snaps, tgt
                    )))
                }
            }
        });

        // Retention on both sides, using the same tiered engine as restic.
        for ds in [&src, &tgt] {
            let step_name = format!("prune {}", ds);
            step!(runner, &step_name, {
                let out = exec
                    .run(&Cmd::new(
                        "zfs",
                        &[
                            "list", "-H", "-t", "snapshot", "-o", "name", "-s", "creation", "-r",
                            ds,
                        ],
                        120,
                    ))
                    .await?;
                // Prune per dataset: recursive snapshots share our name, so
                // grouping by full name keeps parents and children in step.
                let mut victims: Vec<String> = Vec::new();
                let names: Vec<String> = out
                    .stdout
                    .lines()
                    .filter_map(|l| l.split_whitespace().next())
                    .filter(|n| n.contains(&format!("@{}", SNAP_PREFIX)))
                    .map(|n| n.to_string())
                    .collect();
                // One retention decision per dataset, applied to its own snaps.
                let mut by_ds: std::collections::BTreeMap<String, Vec<(String, u64)>> =
                    Default::default();
                for full in &names {
                    let Some((d, s)) = full.split_once('@') else {
                        continue;
                    };
                    by_ds
                        .entry(d.to_string())
                        .or_default()
                        .push((full.clone(), snap_time(s, ctx.now_unix)));
                }
                for (_, snaps) in by_ds {
                    victims.extend(crate::retention::forget_list(&snaps, tiers, ctx.now_unix));
                }
                for v in &victims {
                    // Never recursive: each snapshot was listed explicitly.
                    let _ = exec.run(&Cmd::new("zfs", &["destroy", v], 300)).await?;
                }
                if victims.is_empty() {
                    Ok(StepOutcome::Unchanged)
                } else {
                    Ok(StepOutcome::Changed)
                }
            });
        }

        runner.log(
            Level::Info,
            format!("[zfs] {} → {} replicated at {}", src, tgt, label),
        );
    }

    runner.finish_ok()
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
