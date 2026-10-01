//! fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): a second
//! copy of every repository, and a check that the copies can be read.
//!
//! Every restic repository lived in one place, `rclone:gdrive:homelab-backups`,
//! and nothing ever ran `restic check` on it. A locked or full Google account,
//! a pruning bug like F285, or one corrupt pack would each have left /appdata
//! (20 GB on the NVMe, measured 2026-09-27) with no copy anyone could restore
//! from, and the corruption would have surfaced at the one restore that
//! mattered.
//!
//! Kenny's choice (deep-dive, `lokale-kopie-hdd4tb`): after each night's
//! backups, `restic copy` every repository into a twin on a dataset of the
//! HDD4TB pool, declared in host.toml (`second_copy_dataset`). That pool is a
//! different disk from the data, and the nightly ZFS replication (E8, `zfs
//! send -R`) carries the dataset to HDD18TB as well. Same password file, same
//! retention as the source. Then one repository per night is checked, both
//! copies, and once a month per repository the check also reads a slice of
//! the data. Fire and theft stay an open risk: every disk is in one case.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok};
use crate::ops::backup::{BackupCfg, init_repository, owner_groups, parse_snapshots_json, restic};
use crate::ops::fleetcheck::{Finding, Severity};
use crate::retention::RetentionTier;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;
use crate::state::{HostState, StateStore};

use super::OpCtx;

/// How long a check counts for: one night, with the same 20-hour slack as the
/// backup and the restore drill (the tick lands a few minutes early some
/// nights). Configurable per standing rule 27; the host passes its own.
pub const DEFAULT_CHECK_INTERVAL_S: u64 = 20 * 3600;

/// How often a repository's check also reads data: monthly, Kenny's word.
pub const DEFAULT_DATA_READ_INTERVAL_S: u64 = 30 * 86_400;

/// The data is read in this many slices (`--read-data-subset=n/10`), one per
/// data read, so ten months walk every pack of a repository once instead of
/// sampling the same random tenth for ever.
pub const DATA_SUBSETS: u32 = 10;

/// Older than this, the second copy has stopped: two nights.
pub const COPY_MAX_AGE_S: u64 = 48 * 3600;

/// The most of one failure's text kept in state and shown in a finding.
const REASON_CAP: usize = 300;

/// One repository to copy, and the retention its SOURCE keeps.
///
/// `None` means the source is never pruned, and then neither is the copy:
/// pruning only the copy would forget snapshots the source still holds, and
/// the next night's `restic copy` would bring the same snapshots back again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPolicy {
    pub repo: String,
    pub tiers: Option<Vec<RetentionTier>>,
}

/// Every repository the nightly round writes to, with the retention its
/// writer applies. Sorted by name, one entry per repository.
///
/// A stack that has never been backed up is left out: its repository does
/// not exist yet, and copying from nothing would only report a false failure
/// until its first backup. The next night after that picks it up.
pub fn repo_policies(
    state: &HostState,
    devices: &[String],
    fleet: &[RetentionTier],
) -> Vec<RepoPolicy> {
    let mut out: Vec<RepoPolicy> = Vec::new();
    let mut add = |repo: String, tiers: Option<Vec<RetentionTier>>| {
        if !out.iter().any(|p| p.repo == repo) {
            out.push(RepoPolicy { repo, tiers });
        }
    };
    for st in state.stacks.values() {
        if st.last_backup == 0 {
            continue;
        }
        if st.is_native() {
            // fix-113: `backup_native` keeps the stack file's own policy too.
            let tiers = st
                .manifest
                .as_ref()
                .and_then(|m| m.retention.clone())
                .unwrap_or_else(|| fleet.to_vec());
            // fix-115: a stateless unit has no repository to copy.
            for unit in crate::ops::restoredrill::backed_up_units(&st.natives) {
                add(unit, Some(tiers.clone()));
            }
            continue;
        }
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        // W2: the stack file's own policy wins, as in `backup`.
        let tiers = m.retention.clone().unwrap_or_else(|| fleet.to_vec());
        for (owner, _) in owner_groups(m) {
            add(owner, Some(tiers.clone()));
        }
    }
    // fix-111: host-meta is pruned by the fleet-wide tiers.
    if state.last_host_meta > 0 {
        add(
            crate::ops::restoredrill::HOST_META_REPO.to_string(),
            Some(fleet.to_vec()),
        );
    }
    // The device configurations are never pruned at source.
    for d in devices {
        add(d.clone(), None);
    }
    out.sort_by(|a, b| a.repo.cmp(&b.repo));
    out
}

/// What `zfs list -H -o mounted,mountpoint <dataset>` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dataset {
    Missing,
    /// It exists and is not mounted, or has no mountpoint of its own.
    NotMounted,
    Mounted(String),
}

pub fn parse_dataset(stdout: &str, code: i32) -> Dataset {
    if code != 0 {
        return Dataset::Missing;
    }
    let line = stdout.lines().next().unwrap_or("").trim();
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("yes"), Some(path)) if path.starts_with('/') && path != "/" => {
            Dataset::Mounted(path.to_string())
        }
        (Some(_), Some(_)) => Dataset::NotMounted,
        _ => Dataset::Missing,
    }
}

/// A dataset name the host may create: `pool/name`, nothing a shell or zfs
/// would read as something else.
fn dataset_name_ok(ds: &str) -> bool {
    let ds = ds.trim();
    ds.contains('/')
        && !ds.starts_with('/')
        && !ds.ends_with('/')
        && !ds.contains("..")
        && ds
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ':'))
}

async fn probe_dataset(exec: &dyn Executor, ds: &str) -> Result<Dataset, CoreError> {
    let out = exec
        .run(&Cmd::new(
            "zfs",
            &["list", "-H", "-o", "mounted,mountpoint", ds],
            30,
        ))
        .await?;
    Ok(parse_dataset(&out.stdout, out.code))
}

/// Where the second copy lives, making the declared dataset when it is not
/// there yet. An unmounted dataset is refused: a repository written into the
/// empty mountpoint of a pool that was not imported lands on the NVMe it is
/// meant to be independent of, and fills the disk the host runs from.
async fn ensure_dataset(exec: &dyn Executor, ds: &str, create: bool) -> Result<String, CoreError> {
    if !dataset_name_ok(ds) {
        return Err(CoreError::Validation(format!(
            "second_copy_dataset '{}' is not a dataset name of the form pool/name",
            ds
        )));
    }
    let mut found = probe_dataset(exec, ds).await?;
    if found == Dataset::Missing && create {
        run_ok(exec, &Cmd::new("zfs", &["create", ds], 60)).await?;
        found = probe_dataset(exec, ds).await?;
    }
    match found {
        Dataset::Mounted(path) => Ok(path),
        Dataset::Missing => Err(CoreError::Other(format!(
            "the dataset {} for the second copy does not exist{}",
            ds,
            if create {
                " and could not be created"
            } else {
                ""
            }
        ))),
        Dataset::NotMounted => Err(CoreError::SafetyAbort(format!(
            "the dataset {} is not mounted :: refusing to write repositories into its empty \
             mountpoint, which would put them on the root disk. Import the pool or mount the \
             dataset (`zfs mount {}`)",
            ds, ds
        ))),
    }
}

/// A failure as one short line for state and findings: what restic said,
/// not the whole command line in front of it (the transcript has that).
fn reason(e: &CoreError) -> String {
    let text = match e {
        CoreError::Command { detail, .. } => detail.clone(),
        other => other.to_string(),
    };
    let masked = crate::executor::mask_secrets(&text);
    let mut s: String = masked.chars().take(REASON_CAP).collect();
    if masked.chars().count() > REASON_CAP {
        s.push('…');
    }
    s
}

/// Copy one repository and apply its source's retention to the copy.
async fn copy_one(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    base: &str,
    p: &RepoPolicy,
    now: u64,
) -> Result<StepOutcome, CoreError> {
    let pw = cfg.password_file.as_str();
    let src = format!("{}/{}-config", cfg.restic_base, p.repo);
    // The copy is made with the source's chunker parameters, or it would
    // store every file in different chunks and deduplicate nothing against
    // its own history.
    let init = restic(
        base,
        &p.repo,
        pw,
        &[
            "init",
            "--from-repo",
            &src,
            "--from-password-file",
            pw,
            "--copy-chunker-params",
        ],
        300,
    );
    init_repository(exec, &init, &p.repo).await?;
    // A power cut in the middle of last night's copy leaves a lock; restic
    // only removes locks of processes that are gone, so this is always safe.
    let _ = exec.run(&restic(base, &p.repo, pw, &["unlock"], 120)).await;
    run_ok(
        exec,
        &restic(
            base,
            &p.repo,
            pw,
            &["copy", "--from-repo", &src, "--from-password-file", pw],
            cfg.snapshot_timeout_s,
        ),
    )
    .await?;
    let Some(tiers) = &p.tiers else {
        return Ok(StepOutcome::Changed);
    };
    let listing = run_ok(
        exec,
        &restic(base, &p.repo, pw, &["snapshots", "--json"], 300),
    )
    .await?;
    let doomed = crate::retention::forget_list(&parse_snapshots_json(&listing.stdout), tiers, now);
    if !doomed.is_empty() {
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        run_ok(exec, &restic(base, &p.repo, pw, &args, 900)).await?;
    }
    Ok(StepOutcome::Changed)
}

/// The nightly copy of every repository into the second repository set.
/// One repository failing does not stop the others; each outcome is kept in
/// state, and any failure fails the operation so it is reported that night.
pub async fn copy_all(
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
    dataset: &str,
    repos: &[RepoPolicy],
) -> OperationReport {
    let mut runner = Runner::new("second-copy", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let store = StateStore::new(ctx.exec, &ctx.state_dir);
    let now = ctx.now_unix;

    let mut base = String::new();
    let found = runner
        .step("second-copy dataset", || async {
            base = ensure_dataset(exec, dataset, true).await?;
            Ok(StepOutcome::Unchanged)
        })
        .await;
    if let Err(e) = found {
        // Every repository missed tonight's copy, and the finding says so
        // through the age of the last one that worked.
        let _ = store.update(|s| s.last_second_copy = now).await;
        return runner.finish_err("second-copy dataset", &e);
    }

    let mut failed: Vec<String> = Vec::new();
    for p in repos {
        let name = format!("copy {}", p.repo);
        let res = runner
            .step(&name, || async { copy_one(exec, cfg, &base, p, now).await })
            .await;
        let why = res.err().map(|e| reason(&e));
        if why.is_some() {
            failed.push(p.repo.clone());
        }
        let recorded = store
            .update(|s| {
                let rec = s.second_copies.entry(p.repo.clone()).or_default();
                rec.last_attempt = now;
                if why.is_none() {
                    rec.last_ok = now;
                }
                rec.last_error = why;
            })
            .await;
        if let Err(e) = recorded {
            runner.log(
                Level::Warn,
                format!("[second-copy] could not record {} :: {}", p.repo, e),
            );
        }
    }
    let wanted: Vec<String> = repos.iter().map(|p| p.repo.clone()).collect();
    if let Err(e) = store
        .update(|s| {
            // A repository that left the rotation takes its record with it,
            // so a retired stack's last error does not stand for ever.
            s.second_copies.retain(|k, _| wanted.contains(k));
            s.last_second_copy = now;
        })
        .await
    {
        runner.log(
            Level::Warn,
            format!("[second-copy] could not record the night :: {}", e),
        );
    }
    if !failed.is_empty() {
        return runner.finish_err(
            "copy",
            &CoreError::Other(format!(
                "{} of {} repositories were not copied to {}: {}",
                failed.len(),
                repos.len(),
                dataset,
                failed.join(", ")
            )),
        );
    }
    runner.log(
        Level::Info,
        format!(
            "[second-copy] {} repositories copied to {} ({})",
            repos.len(),
            dataset,
            base
        ),
    );
    runner.finish_ok()
}

/// Whose turn it is tonight: the repository checked longest ago, never
/// checked first, ties by name.
pub fn pick(state: &HostState, repos: &[String]) -> Option<String> {
    let mut sorted = repos.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
        .into_iter()
        .min_by_key(|r| state.integrity.get(r).map(|i| i.last_check).unwrap_or(0))
}

/// Does this repository's check read data tonight, and which slice?
pub fn data_subset(state: &HostState, repo: &str, now: u64, interval_s: u64) -> Option<(u32, u32)> {
    let rec = state.integrity.get(repo);
    let last = rec.map(|r| r.last_data_read).unwrap_or(0);
    if last != 0 && now.saturating_sub(last) < interval_s {
        return None;
    }
    let n = rec.map(|r| r.next_subset).unwrap_or(0);
    let n = if (1..=DATA_SUBSETS).contains(&n) {
        n
    } else {
        1
    };
    Some((n, DATA_SUBSETS))
}

/// Should tonight's check include the second copy of this repository? Only
/// once a copy of it has completed: before that there is nothing to check,
/// and the copy's own failure is already a finding.
pub fn check_local(state: &HostState, repo: &str) -> bool {
    state.second_copies.get(repo).is_some_and(|c| c.last_ok > 0)
}

async fn check_one(
    exec: &dyn Executor,
    base: &str,
    repo: &str,
    pw: &str,
    subset: Option<(u32, u32)>,
    timeout: u64,
) -> Result<StepOutcome, CoreError> {
    let _ = exec.run(&restic(base, repo, pw, &["unlock"], 120)).await;
    let flag = subset.map(|(n, t)| format!("--read-data-subset={}/{}", n, t));
    let mut args: Vec<&str> = vec!["check"];
    if let Some(f) = &flag {
        args.push(f);
    }
    let out = exec.run(&restic(base, repo, pw, &args, timeout)).await?;
    if out.success() {
        return Ok(StepOutcome::Unchanged);
    }
    // restic prints the damage on stdout and the verdict on stderr.
    let tail: Vec<&str> = out
        .stderr
        .lines()
        .chain(out.stdout.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    Err(CoreError::Other(format!(
        "restic check of {}/{}-config exited {}: {}",
        base,
        repo,
        out.code,
        tail.iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ")
    )))
}

/// Check one repository, on Google Drive and (when `dataset` is given) its
/// second copy. Both are checked whatever the first one says, the outcome of
/// each is kept in state, and a failure of either fails the operation.
pub async fn check_repo(
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
    repo: &str,
    dataset: Option<&str>,
    subset: Option<(u32, u32)>,
) -> OperationReport {
    let mut runner = Runner::new(&format!("check-{}", repo), ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let pw = cfg.password_file.as_str();
    let timeout = cfg.snapshot_timeout_s;

    let drive = runner
        .step("check the Google Drive copy", || async {
            check_one(exec, &cfg.restic_base, repo, pw, subset, timeout).await
        })
        .await;
    let drive_error = drive.err().map(|e| reason(&e));

    let mut local_error = None;
    if let Some(ds) = dataset {
        let local = runner
            .step("check the second copy", || async {
                let base = ensure_dataset(exec, ds, false).await?;
                check_one(exec, &base, repo, pw, subset, timeout).await
            })
            .await;
        local_error = local.err().map(|e| reason(&e));
    }

    let now = ctx.now_unix;
    let (d, l) = (drive_error.clone(), local_error.clone());
    let recorded = StateStore::new(ctx.exec, &ctx.state_dir)
        .update(|s| {
            let rec = s.integrity.entry(repo.to_string()).or_default();
            rec.last_check = now;
            if let Some((n, t)) = subset {
                rec.last_data_read = now;
                rec.next_subset = n % t + 1;
            }
            rec.drive_error = d;
            rec.local_error = l;
            s.last_integrity_check = now;
        })
        .await;
    if let Err(e) = recorded {
        runner.log(
            Level::Warn,
            format!("[check] could not record the check of {} :: {}", repo, e),
        );
    }

    let failed: Vec<String> = [
        ("Google Drive", &drive_error),
        ("second copy", &local_error),
    ]
    .iter()
    .filter_map(|(side, e)| e.as_ref().map(|e| format!("{}: {}", side, e)))
    .collect();
    if !failed.is_empty() {
        return runner.finish_err(
            "restic check",
            &CoreError::Other(format!(
                "{}-config did not pass restic check :: {}",
                repo,
                failed.join(" | ")
            )),
        );
    }
    runner.log(
        Level::Info,
        format!(
            "[check] {}-config passed restic check{}{}",
            repo,
            if dataset.is_some() {
                " on both copies"
            } else {
                ""
            },
            subset
                .map(|(n, t)| format!(", data slice {}/{} read", n, t))
                .unwrap_or_default()
        ),
    );
    runner.finish_ok()
}

/// What the state says about the second copy and the checks. `dataset` is
/// the configured `second_copy_dataset`; None = no second copy is configured,
/// and then nothing is said about one.
pub fn evaluate_copies(
    state: &HostState,
    now: u64,
    dataset: Option<&str>,
    check_interval_s: u64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let when = |t: u64| {
        if t == 0 {
            "never".to_string()
        } else {
            crate::state::ymd(t)
        }
    };
    if let Some(ds) = dataset {
        for (repo, rec) in &state.second_copies {
            let Some(err) = &rec.last_error else {
                continue;
            };
            out.push(Finding {
                severity: Severity::Broken,
                subject: format!("second copy · {}", repo),
                what: format!(
                    "the copy of {}-config to {} failed: {} (last copied: {})",
                    repo,
                    ds,
                    err,
                    when(rec.last_ok)
                ),
                remedy: "this repository exists on Google Drive only until it copies again; \
                         read the night's second-copy transcript, the nightly round retries"
                    .into(),
            });
        }
        let newest = state
            .second_copies
            .values()
            .map(|r| r.last_ok)
            .max()
            .unwrap_or(0);
        if state.last_second_copy == 0 {
            out.push(Finding {
                severity: Severity::Drift,
                subject: "second copy".into(),
                what: format!("the second copy on {} has not run yet", ds),
                remedy: "the nightly round makes it after the backups; if this stands a day, \
                         the round is not reaching it"
                    .into(),
            });
        } else if now.saturating_sub(newest) > COPY_MAX_AGE_S {
            out.push(Finding {
                severity: Severity::Broken,
                subject: "second copy".into(),
                what: format!(
                    "no repository has been copied to {} since {}",
                    ds,
                    when(newest)
                ),
                remedy: "every backup exists on Google Drive only; check that the pool is \
                         imported and the dataset mounted, then read the second-copy transcript"
                    .into(),
            });
        }
    }
    for (repo, rec) in &state.integrity {
        for (side, err) in [
            ("Google Drive", &rec.drive_error),
            ("second copy", &rec.local_error),
        ] {
            let Some(err) = err else {
                continue;
            };
            out.push(Finding {
                severity: Severity::Broken,
                subject: format!("restic check · {} ({})", repo, side),
                what: format!(
                    "{}-config did not pass restic check on {}: {}",
                    repo,
                    when(rec.last_check),
                    err
                ),
                remedy: "a restore from this copy may fail; run `restic check` on it by hand, \
                         and restore from the other copy if one is damaged"
                    .into(),
            });
        }
    }
    // A night's grace on top of the interval, as for the restore drill.
    if state.last_integrity_check == 0
        || now.saturating_sub(state.last_integrity_check)
            > check_interval_s.saturating_add(24 * 3600)
    {
        out.push(Finding {
            severity: Severity::Drift,
            subject: "restic check".into(),
            what: format!(
                "no repository has been checked since: {}",
                when(state.last_integrity_check)
            ),
            remedy: "the nightly round checks one repository per night; if this stands, the \
                     round is not reaching it"
                .into(),
        });
    }
    out
}
