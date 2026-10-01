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
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok, shq};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

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

// fix-85 (expert panel, zfs-replica-mirrors-mistakes, 2026-09-27): the
// replica keeps its own history.
//
// Until then every job was ONE `zfs send -RI | zfs receive -F`. A
// replication stream (-R) received with -F destroys on the target every
// snapshot and file system that no longer exists on the source
// (zfs-receive(8)), so the replica was a mirror: measured on pve that day,
// every replica dataset held exactly its source's snapshots and no hold. A
// mistaken `zfs destroy -r HDD4TB/backups` would have reached
// HDD18TB/replica/HDD4TB/backups the next night.
//
// Now every dataset of the source subtree is sent on its own, without -R, so
// no stream carries the source's idea of what should exist. What the source
// lost stays on the replica: a dataset the source no longer has is an
// orphan, left untouched and never pruned; a snapshot the source destroyed
// is kept until the replica's OWN retention (longer than the source's)
// thins it.

/// What one run does with one dataset of a job's subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetStep {
    /// Send everything from `base` up to the new snapshot. `base` is the
    /// target's newest snapshot, never an older one (see `plan_replication`).
    Incremental { base: String },
    /// The target dataset does not exist yet: a full send creates it.
    Seed,
    /// The target exists with no snapshot anywhere in its subtree, so there
    /// is no history on it to lose: a full send over it.
    SeedEmpty,
    /// The target already holds the new snapshot.
    UpToDate,
    /// Not sent; the replica is left exactly as it is. The run fails with
    /// this text, after the other datasets were sent.
    Refuse(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetPlan {
    pub source: String,
    pub target: String,
    pub step: DatasetStep,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplicaPlan {
    /// One entry per source dataset, parents before their children.
    pub datasets: Vec<DatasetPlan>,
    /// Target datasets whose source dataset no longer exists. Never sent
    /// into, never pruned: they are the copy of what the source lost.
    pub orphans: Vec<String>,
}

/// `zfs list` names grouped per dataset, snapshot names in listing order
/// (oldest first when listed with `-s creation`). A line without `@` names a
/// dataset with no snapshots; it is kept as an empty entry.
fn group_by_dataset(list_stdout: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for name in list_stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
    {
        match name.split_once('@') {
            Some((ds, snap)) => out
                .entry(ds.to_string())
                .or_default()
                .push(snap.to_string()),
            None => {
                out.entry(name.to_string()).or_default();
            }
        }
    }
    out
}

/// `root` or a dataset under it, rebased onto `onto`.
fn rebase(ds: &str, root: &str, onto: &str) -> Option<String> {
    if ds == root {
        Some(onto.to_string())
    } else {
        ds.strip_prefix(&format!("{}/", root))
            .map(|rest| format!("{}/{}", onto, rest))
    }
}

/// Decide, per dataset, what one run sends. Pure: the inputs are the stdout
/// of `zfs list -H -t snapshot -o name -s creation -r <source>` (taken after
/// the run's own `zfs snapshot -r`, so every source dataset appears),
/// `zfs list -H -o name -t filesystem,volume -r <target>` (empty when the
/// target does not exist), and the snapshot listing of the target.
pub fn plan_replication(
    job: &ZfsJob,
    label: &str,
    source_snaps: &str,
    target_datasets: &str,
    target_snaps: &str,
) -> ReplicaPlan {
    let src = group_by_dataset(source_snaps);
    let tgt = group_by_dataset(target_snaps);
    let mut exists: std::collections::BTreeSet<String> = tgt.keys().cloned().collect();
    exists.extend(group_by_dataset(target_datasets).into_keys());

    let mut plan = ReplicaPlan::default();
    // BTreeMap order puts a parent before its children (a name sorts before
    // every name it is a prefix of), so a seeded parent exists by the time
    // its child is received.
    for (s_ds, s_snaps) in &src {
        let Some(t_ds) = rebase(s_ds, &job.source, &job.target) else {
            continue;
        };
        let empty = Vec::new();
        let t_snaps = tgt.get(&t_ds).unwrap_or(&empty);
        let refused_parent = plan.datasets.iter().find(|p| {
            matches!(p.step, DatasetStep::Refuse(_)) && t_ds.starts_with(&format!("{}/", p.target))
        });
        let step = if let Some(p) = refused_parent {
            DatasetStep::Refuse(format!(
                "{} is not sent because its parent {} was not replicated this run",
                s_ds, p.source
            ))
        } else if !exists.contains(&t_ds) {
            DatasetStep::Seed
        } else if t_snaps.is_empty() {
            // Emptiness is a property of the whole SUBTREE: the retired
            // script's retention deleted parent snapshots while children kept
            // theirs, so a populated replica can present an empty parent.
            let subtree: usize = tgt
                .iter()
                .filter(|(d, _)| d.starts_with(&format!("{}/", t_ds)))
                .map(|(_, v)| v.len())
                .sum();
            if subtree == 0 {
                DatasetStep::SeedEmpty
            } else {
                DatasetStep::Refuse(no_common_base(s_ds, &t_ds, subtree))
            }
        } else {
            let newest = t_snaps.last().expect("checked non-empty");
            if newest == label {
                DatasetStep::UpToDate
            } else if s_snaps.contains(newest) {
                DatasetStep::Incremental {
                    base: newest.clone(),
                }
            } else if let Some(shared) = common_base(s_snaps, t_snaps) {
                // The source lost the replica's newest snapshot (destroyed by
                // hand, or rolled back). An incremental from the older shared
                // one can only be received by rolling the replica back to it,
                // which destroys the replica's newer snapshots: the very thing
                // fix-85 exists to prevent. Stop and leave it to a person.
                DatasetStep::Refuse(format!(
                    "{t}@{n} is the replica's newest snapshot and {s} no longer has it. The \
                     newest snapshot both still share is {b}; receiving from there would roll \
                     {t} back and destroy {t}@{n} and every snapshot after {b}. {s} is not \
                     replicated, and {t} is kept exactly as it is. Decide deliberately: find \
                     out why the source lost {n}; to accept losing it on the replica too, run \
                     `zfs rollback -r {t}@{b}` yourself and re-run.",
                    t = t_ds,
                    n = newest,
                    s = s_ds,
                    b = shared
                ))
            } else {
                let subtree: usize = tgt
                    .iter()
                    .filter(|(d, _)| *d == &t_ds || d.starts_with(&format!("{}/", t_ds)))
                    .map(|(_, v)| v.len())
                    .sum();
                DatasetStep::Refuse(no_common_base(s_ds, &t_ds, subtree))
            }
        };
        plan.datasets.push(DatasetPlan {
            source: s_ds.clone(),
            target: t_ds,
            step,
        });
    }
    plan.orphans = exists
        .into_iter()
        .filter(|t| {
            rebase(t, &job.target, &job.source)
                .map(|s| !src.contains_key(&s))
                .unwrap_or(false)
        })
        .collect();
    plan
}

fn no_common_base(src: &str, tgt: &str, snaps: usize) -> String {
    // The dangerous case the old script powered through.
    format!(
        "{} and {} share no snapshot, but {} already holds {} snapshot(s) (subtree \
         included). Re-seeding would destroy that history, so this job stops here. Decide \
         deliberately: investigate why the chain broke, or wipe the target yourself with \
         `zfs destroy -r {}` and re-run for a fresh seed.",
        src, tgt, tgt, snaps, tgt
    )
}

/// The replica's own tiers (fix-85): denser than the source's default at
/// every age (daily for two weeks, weekly for about four months, then monthly
/// forever), because HDD18TB is where history is meant to outlive the
/// source's shorter memory.
pub fn replica_tiers() -> Vec<crate::retention::RetentionTier> {
    use crate::retention::RetentionTier;
    vec![
        RetentionTier {
            every_days: 1,
            span_days: Some(14),
        },
        RetentionTier {
            every_days: 7,
            span_days: Some(120),
        },
        RetentionTier {
            every_days: 30,
            span_days: None,
        },
    ]
}

/// What the replica forgets: only what BOTH its own tiers and the source's
/// tiers would forget. The source's tiers are live settings; taking the
/// intersection means a longer source policy can never make the replica the
/// shorter memory of the two. `forget_list` is used as it is (fix-42).
pub fn replica_forget(
    snapshots: &[(String, u64)],
    source_tiers: &[crate::retention::RetentionTier],
    now: u64,
) -> Vec<String> {
    let by_source = crate::retention::forget_list(snapshots, source_tiers, now);
    crate::retention::forget_list(snapshots, &replica_tiers(), now)
        .into_iter()
        .filter(|id| by_source.contains(id))
        .collect()
}

/// Full snapshot names (`ds@homelab-…`) to destroy, per dataset of a
/// snapshot listing, deciding with `forget`. Only our own prefix is ever a
/// candidate; datasets not in `only` (when given) are skipped entirely.
fn prune_victims(
    list_stdout: &str,
    only: Option<&std::collections::BTreeSet<String>>,
    now: u64,
    forget: impl Fn(&[(String, u64)]) -> Vec<String>,
) -> Vec<String> {
    let mut victims = Vec::new();
    for (ds, snaps) in group_by_dataset(list_stdout) {
        if only.is_some_and(|o| !o.contains(&ds)) {
            continue;
        }
        // One retention decision per dataset, applied to its own snaps.
        let ours: Vec<(String, u64)> = snaps
            .iter()
            .filter(|s| s.starts_with(SNAP_PREFIX))
            .map(|s| (format!("{}@{}", ds, s), snap_time(s, now)))
            .collect();
        victims.extend(forget(&ours));
    }
    victims
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

        let list_snaps = |ds: &str| {
            let ds = ds.to_string();
            async move {
                exec.run(&Cmd::new(
                    "zfs",
                    &[
                        "list", "-H", "-t", "snapshot", "-o", "name", "-s", "creation", "-r", &ds,
                    ],
                    120,
                ))
                .await
            }
        };

        // Filled by the replicate step for what follows it: the target
        // datasets this run replicated (only those are pruned, with the
        // replica's tiers) and the ones whose source is gone.
        let mut replicated: std::collections::BTreeSet<String> = Default::default();
        let mut orphans: Vec<String> = Vec::new();

        let step_name = format!("replicate {} → {}", src, tgt);
        step!(runner, &step_name, {
            let src_out = list_snaps(&src).await?;
            let tgt_out = list_snaps(&tgt).await?;
            let tgt_ds_out = exec
                .run(&Cmd::new(
                    "zfs",
                    &[
                        "list",
                        "-H",
                        "-o",
                        "name",
                        "-t",
                        "filesystem,volume",
                        "-r",
                        &tgt,
                    ],
                    120,
                ))
                .await?;
            // A target that does not exist fails both listings; it is then
            // absent from the plan's view and gets seeded.
            let or_empty = |o: &crate::executor::CmdOutput| {
                if o.success() {
                    o.stdout.clone()
                } else {
                    String::new()
                }
            };
            let plan = plan_replication(
                job,
                &label,
                &src_out.stdout,
                &or_empty(&tgt_ds_out),
                &or_empty(&tgt_out),
            );
            orphans = plan.orphans.clone();

            let mut refusals: Vec<String> = Vec::new();
            let mut changed = false;
            for d in &plan.datasets {
                let new = format!("{}@{}", d.source, label);
                // fix-85: never `-R`. Each stream carries one dataset and no
                // opinion about what else should exist on the target, so
                // `receive` has nothing to delete. The stream carries no
                // properties either (that came with -R); a new replica
                // dataset inherits HDD18TB's.
                //
                // F177: `-x mountpoint` stays on every receive. A replica
                // must never arrive claiming the live path its source is
                // mounted at: found 2026-09-02 while following the DR
                // runbook, `HDD18TB/replica/HDD2TB/paperless-config` and the
                // real one both had mountpoint=/appdata/paperwork/
                // paperless-config with canmount=on, and nothing decides
                // which wins after a reboot.
                let (script, timeout) = match &d.step {
                    DatasetStep::Incremental { base } => (
                        // `-F` rolls back stray writes made on the mounted
                        // replica since its newest snapshot. On a plain
                        // stream it would also destroy target snapshots
                        // newer than the base, which is why the plan only
                        // ever takes the target's newest snapshot as base
                        // and refuses otherwise.
                        format!(
                            "zfs send -I {} {} | zfs receive -F -x mountpoint {}",
                            shq(&format!("{}@{}", d.source, base)),
                            shq(&new),
                            shq(&d.target)
                        ),
                        6 * 3600,
                    ),
                    DatasetStep::Seed => (
                        format!(
                            "zfs send {} | zfs receive -x mountpoint {}",
                            shq(&new),
                            shq(&d.target)
                        ),
                        12 * 3600,
                    ),
                    // Exists with no snapshot in its whole subtree: nothing
                    // on it to lose, and a full stream needs -F to land on an
                    // existing dataset.
                    DatasetStep::SeedEmpty => (
                        format!(
                            "zfs send {} | zfs receive -F -x mountpoint {}",
                            shq(&new),
                            shq(&d.target)
                        ),
                        12 * 3600,
                    ),
                    DatasetStep::UpToDate => {
                        replicated.insert(d.target.clone());
                        continue;
                    }
                    DatasetStep::Refuse(why) => {
                        refusals.push(why.clone());
                        continue;
                    }
                };
                run_ok(exec, &Cmd::new("sh", &["-c", &script], timeout)).await?;
                replicated.insert(d.target.clone());
                changed = true;
            }
            if !refusals.is_empty() {
                // The other datasets were sent; the refused ones stay as they
                // are and the night fails, so a person looks at them.
                return Err(CoreError::SafetyAbort(refusals.join(" ")));
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });

        for o in &orphans {
            runner.log(
                Level::Warn,
                format!(
                    "[zfs] {} is kept although its source {} no longer exists; it is never \
                     sent into or pruned. Destroy it by hand once it is no longer wanted.",
                    o,
                    rebase(o, &tgt, &src).unwrap_or_default()
                ),
            );
        }

        // Retention: the source with its own tiers, the replica with its
        // longer ones (fix-85), each deciding per dataset with forget_list.
        let step_name = format!("prune {}", src);
        step!(runner, &step_name, {
            let out = list_snaps(&src).await?;
            let victims = prune_victims(&out.stdout, None, ctx.now_unix, |s| {
                crate::retention::forget_list(s, tiers, ctx.now_unix)
            });
            destroy_each(exec, &victims).await
        });
        let step_name = format!("prune {}", tgt);
        step!(runner, &step_name, {
            let out = list_snaps(&tgt).await?;
            // Only what this run replicated: an orphan is the last copy of
            // something the source lost, and a refused dataset waits for a
            // person as it is.
            let victims = prune_victims(&out.stdout, Some(&replicated), ctx.now_unix, |s| {
                replica_forget(s, tiers, ctx.now_unix)
            });
            destroy_each(exec, &victims).await
        });

        runner.log(
            Level::Info,
            format!("[zfs] {} → {} replicated at {}", src, tgt, label),
        );
    }

    runner.finish_ok()
}

async fn destroy_each(exec: &dyn Executor, victims: &[String]) -> Result<StepOutcome, CoreError> {
    for v in victims {
        // Never recursive: each snapshot was listed explicitly.
        let _ = exec.run(&Cmd::new("zfs", &["destroy", v], 300)).await?;
    }
    Ok(if victims.is_empty() {
        StepOutcome::Unchanged
    } else {
        StepOutcome::Changed
    })
}
