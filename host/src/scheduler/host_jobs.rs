//! The night's host-level jobs: the host's own state, the restore drill, devices, the home address, the second copy, the integrity check and ZFS.

use super::Night;
use crate::*;

/// H10: the host's own crown jewels.
pub(super) async fn host_meta(n: &Night<'_>) {
    let (state, exec, store, tiers) = (n.state, n.exec, &n.store, &n.tiers);
    let now = n.now;
    let plan = &n.plan;
    if plan.contains(&NightlyTask::HostMeta) {
        let cfg = homelab_core::ops::backup::BackupCfg {
            tiers: tiers.clone(),
            ..state.config.backup.clone()
        };
        let report = run_mutating_op(state, exec, 0, "host-meta-backup", |ctx| {
            Box::pin(async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await })
        })
        .await;
        if report.ok {
            record_state(store, "last_host_meta", |s| s.last_host_meta = now).await;
        } else {
            tracing::error!(
                "scheduler: host-meta backup FAILED — the vault/state/TLS snapshot is the recovery path for a lost host disk; investigate now"
            );
        }
    }
}

/// G14: one restore, rehearsed for real, in turn.
pub(super) async fn restore_drill(n: &Night<'_>) {
    let (state, exec, store, tiers) = (n.state, n.exec, &n.store, &n.tiers);
    let now = n.now;
    let (plan, drill_pick, drill_repos, pg_stacks) =
        (&n.plan, &n.drill_pick, &n.drill_repos, &n.pg_stacks);
    // G14: one restore, rehearsed for real, in turn.
    //
    // Restores into a temporary directory and judges what came back by
    // its LARGEST file rather than its first. That is not fussiness: on
    // 2026-09-02 a hand-run drill declared a restore identical to live by
    // comparing two md5 sums that both belonged to a zero-byte file, and
    // a drill that can be satisfied by empty files rehearses nothing.
    if plan.contains(&NightlyTask::RestoreDrill)
        && let Some(repo) = drill_pick.clone()
    {
        let cfg = homelab_core::ops::backup::BackupCfg {
            tiers: tiers.clone(),
            ..state.config.backup.clone()
        };
        // fix-62 (see REGISTER.md): a data pool, not the state dir on
        // pve-root.
        let target = format!(
            "{}/restore-drill",
            state.config.restore_drill_scratch_dir.trim_end_matches('/')
        );
        let pg = homelab_core::ops::restoredrill::postgres_check(&pg_stacks, &repo);
        let outcome = run_restore_drill(exec, &cfg, &repo, &target, pg.as_ref()).await;
        match &outcome {
            homelab_core::ops::restoredrill::Outcome::Passed {
                files,
                largest_bytes,
            } => info!(
                "restore drill: {} came back with {} file(s), largest {} bytes",
                repo, files, largest_bytes
            ),
            homelab_core::ops::restoredrill::Outcome::Failed(why) => {
                tracing::error!("restore drill: {} proved nothing :: {}", repo, why)
            }
        }
        record_state(store, "restore drill", |sn| {
            homelab_core::ops::restoredrill::record(sn, &drill_repos, &repo, &outcome, now)
        })
        .await;
    }
}

/// Route A: the devices this suite may not touch, asked for their own configuration.
pub(super) async fn device_config(n: &Night<'_>) {
    let (state, exec, tiers) = (n.state, n.exec, &n.tiers);
    let plan = &n.plan;
    if plan.contains(&NightlyTask::DeviceConfig) {
        for dev in state.config.device_backups.clone() {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            let name = dev.name.clone();
            let report = run_mutating_op(state, exec, 0, "device-backup", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
                })
            })
            .await;
            if !report.ok {
                tracing::error!(
                    "scheduler: device backup for {} FAILED — its configuration is not \
                         being kept",
                    name
                );
            } else {
                record_device_backup(state, &name).await;
            }
        }
    }
}

/// fix-94: keep CrowdSec's whitelist on the gateway equal to the house's public address.
pub(super) async fn home_address(n: &Night<'_>) {
    let (state, exec) = (n.state, n.exec);
    // fix-94: the house's public address is dynamic; keep CrowdSec's
    // whitelist on the gateway equal to it. Cheap when nothing changed:
    // one read of the file, one GET against the router, no write.
    // fix-156: every night, whatever else the night does.
    run_mutating_op(state, exec, 0, "home-address-whitelist", |ctx| {
        Box::pin(async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await })
    })
    .await;
}

/// fix-96: every repository copied into the second repository set.
pub(super) async fn second_copy(n: &Night<'_>) {
    let (state, exec, plan, copy_policies) = (n.state, n.exec, &n.plan, &n.copy_policies);
    // fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): every
    // repository copied into the second repository set, after all of
    // tonight's backups and before the ZFS replication below, so the
    // replica on HDD18TB carries tonight's copy too.
    if plan.contains(&NightlyTask::SecondCopy)
        && let Some(ds) = state.config.second_copy_dataset.clone()
    {
        let cfg = state.config.backup.clone();
        let repos = copy_policies.clone();
        let report = run_mutating_op(state, exec, 0, "second-copy", |ctx| {
            Box::pin(async move {
                homelab_core::ops::secondcopy::copy_all(ctx, &cfg, &ds, &repos).await
            })
        })
        .await;
        if !report.ok {
            tracing::error!(
                "scheduler: the second copy did not complete — the repositories it names \
                         exist on Google Drive only tonight"
            );
        }
    }
}

/// fix-96: one repository checked per night, both copies.
pub(super) async fn integrity_check(n: &Night<'_>) {
    let (state, exec, store, plan, copy_policies) =
        (n.state, n.exec, &n.store, &n.plan, &n.copy_policies);
    let now = n.now;
    // fix-96: one repository checked per night, both copies; once a month
    // per repository the check also reads a slice of the data. Read after
    // the copy above, which records which repositories have a copy yet.
    if plan.contains(&NightlyTask::IntegrityCheck)
        && let Ok(fresh) = store.load().await
    {
        let names: Vec<String> = copy_policies.iter().map(|p| p.repo.clone()).collect();
        if let Some(repo) = homelab_core::ops::secondcopy::pick(&fresh, &names) {
            let subset = homelab_core::ops::secondcopy::data_subset(
                &fresh,
                &repo,
                now,
                state.config.integrity_data_read_interval_s,
            );
            let local = state
                .config
                .second_copy_dataset
                .clone()
                .filter(|_| homelab_core::ops::secondcopy::check_local(&fresh, &repo));
            let cfg = state.config.backup.clone();
            let report = run_mutating_op(state, exec, 0, "restic-check", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::secondcopy::check_repo(
                        ctx,
                        &cfg,
                        &repo,
                        local.as_deref(),
                        subset,
                    )
                    .await
                })
            })
            .await;
            if !report.ok {
                tracing::error!(
                    "scheduler: restic check found a problem — {}",
                    report.message
                );
            }
        }
    }
}

/// E8: ZFS snapshots and replication of the big pools.
pub(super) async fn zfs(n: &Night<'_>) {
    let (state, exec, store, tiers, plan) = (n.state, n.exec, &n.store, &n.tiers, &n.plan);
    let now = n.now;
    // E8: ZFS snapshots + replication of the big pools.
    if plan.contains(&NightlyTask::Zfs) {
        let jobs = state.config.zfs_jobs.clone();
        let tiers = tiers.clone();
        let report = run_mutating_op(state, exec, 0, "zfs-replicate", |ctx| {
            Box::pin(async move {
                homelab_core::ops::zfs::replicate(
                    ctx,
                    &jobs,
                    &tiers,
                    homelab_core::ops::backup::BackupTrigger::Nightly,
                )
                .await
            })
        })
        .await;
        if report.ok {
            record_state(store, "last_zfs", |s| s.last_zfs = now).await;
        } else {
            tracing::error!(
                "scheduler: ZFS replication FAILED — investigate; the old cron script used to fail silently, this one does not"
            );
        }
    }
}
