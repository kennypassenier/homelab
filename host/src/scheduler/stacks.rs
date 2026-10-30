//! Each stack's night: the backup batch, then updates, settings and parking, one stack at a time.

use super::Night;
use crate::*;
use tracing::Instrument as _;

/// Y1: every due backup runs first, several at a time, under one hold of the
/// global lock; the updates then stay serial.
pub(super) async fn backup_batch(
    n: &Night<'_>,
    stacks: &std::collections::BTreeMap<String, homelab_core::state::StackState>,
) -> std::collections::HashMap<String, NightBackup> {
    let (state, exec, tiers, plan) = (n.state, n.exec, &n.tiers, &n.plan);
    let backup_jobs: Vec<BackupJob> = stacks
        .iter()
        .filter(|(name, _)| plan.contains(&NightlyTask::Stack((*name).clone())))
        .filter_map(|(name, st)| {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            // fix-138: the same decision the night tests reach.
            let what = match homelab_core::ops::night::backup_work(st)? {
                homelab_core::ops::night::BackupWork::Native(n) => BackupWhat::Native(n),
                homelab_core::ops::night::BackupWork::Compose(m) => BackupWhat::Compose(m),
            };
            Some(BackupJob {
                stack: name.clone(),
                what,
                cfg,
            })
        })
        .collect();
    let backup_done =
        run_backup_batch(state, exec, backup_jobs, state.config.backup_concurrency).await;
    touch_scheduler_heartbeat(state);
    backup_done
}

/// fix-138: what each stack's night does is decided by `ops::night::stack_night`;
/// this only runs the operations, writes state and logs.
pub(super) async fn stack_night(
    n: &Night<'_>,
    name: String,
    st: homelab_core::state::StackState,
    backup_done: &std::collections::HashMap<String, NightBackup>,
    updates_parked: &std::collections::BTreeMap<String, u64>,
) {
    let (state, exec, store, plan) = (n.state, n.exec, &n.store, &n.plan);
    let now = n.now;
    let due = plan.contains(&NightlyTask::Stack(name.clone()));
    let backup = backup_done
        .get(&name)
        .cloned()
        .unwrap_or(NightBackup::Failed);
    let night = homelab_core::ops::night::stack_night(
        &name,
        &st,
        due,
        updates_parked.contains_key(&name),
        &backup,
    );
    for (level, line) in &night.lines {
        match level {
            homelab_core::sink::Level::Warn | homelab_core::sink::Level::Error => {
                tracing::warn!("{}", line)
            }
            _ => info!("{}", line),
        }
    }
    if night.record_last_backup {
        // Record last_backup so tomorrow's check is accurate.
        record_state(store, "last_backup", |s| {
            if let Some(rec) = s.stacks.get_mut(&name) {
                rec.last_backup = now;
            }
            // fix-nightly-1: only the night's own backup counts as
            // tonight's.
            s.last_nightly_backup.insert(name.clone(), now);
        })
        .await;
    }
    let mut update_ok = true;
    match night.updates {
        homelab_core::ops::night::UpdateWork::None => {}
        homelab_core::ops::night::UpdateWork::Compose(m) => {
            let m2 = *m;
            let r = run_mutating_op(state, exec, 0, "scheduled-update", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::update::update(ctx, &m2, None, true).await },
                )
            })
            .instrument(stack_span(&name))
            .await;
            update_ok = r.ok;
        }
        homelab_core::ops::night::UpdateWork::Native { release, own_cmd } => {
            let applied = Some(st.applied_at);
            // B1: the orchestrator's own release update.
            for native in release {
                let r = run_mutating_op(state, exec, 0, "scheduled-release-update", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::release_update(ctx, &native).await
                    })
                })
                .instrument(stack_span(&name))
                .await;
                update_ok &= r.ok;
            }
            for native in own_cmd {
                let r = run_mutating_op(state, exec, 0, "scheduled-update-native", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::update_native(ctx, &native, applied).await
                    })
                })
                .instrument(stack_span(&name))
                .await;
                update_ok &= r.ok;
            }
        }
    }
    // app-settings-enforced-1: the values a fix depends on, put back
    // in the apps' own files. After the backup batch and the update
    // (both done with this stack), under the same operation lock as
    // every mutation, so never while a backup of the stack runs. A
    // stack whose update just failed is left alone.
    if update_ok
        && st.enabled
        && !st.is_native()
        && let Some(m) = st.manifest.clone()
        && store
            .load()
            .await
            .is_ok_and(|s| homelab_core::ops::settings::stack_has_settings(&s, &name))
    {
        let _ = run_mutating_op(state, exec, 0, "scheduled-settings", |ctx| {
            Box::pin(async move { homelab_core::ops::settings::enforce(ctx, &m).await })
        })
        .instrument(stack_span(&name))
        .await;
    }
    // H8 / fix-59: only a failed update parks, and only the updates;
    // a failed or deferred backup is simply tried again tomorrow.
    if night.settle_park {
        park_after_night(state, exec, store, &name, update_ok, now).await;
    }
    // fix-52 residual: this stack's update work (which can itself
    // run long) finished without the loop ever stalling on it.
    touch_scheduler_heartbeat(state);
}
