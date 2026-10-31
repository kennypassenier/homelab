//! Running a batch of backups and recording what was done in the history.

use crate::*;

pub(crate) async fn run_backup_batch(
    state: &AppState,
    exec: &RealExecutor,
    jobs: Vec<BackupJob>,
    limit: usize,
) -> std::collections::HashMap<String, NightBackup> {
    use futures_util::stream::StreamExt;
    if jobs.is_empty() {
        return std::collections::HashMap::new();
    }
    let limit = effective_concurrency(limit);
    info!(
        "scheduler: backing up {} stack(s), {} at a time",
        jobs.len(),
        limit
    );
    let _guard = lock_ops(state).await;
    let _busy = BusyMark::set(state, "the nightly backup", jobs.len(), None);
    // M-T75: measured, not predicted — the line the morning reads.
    let phase_started = std::time::Instant::now();
    let stacks_in_phase = jobs.len();
    let results: Vec<(String, NightBackup)> = futures_util::stream::iter(jobs)
        .map(|job| {
            // fix-122: the three backups interleave; their lines say whose.
            use tracing::Instrument as _;
            let span = stack_span(&job.stack);
            async move {
                let name = job.stack.clone();
                let outcome = match job.what {
                    BackupWhat::Compose(manifest) => {
                        let mut cfg = job.cfg.clone();
                        // fix-223: this is the nightly round — tag it as such.
                        cfg.trigger = homelab_core::ops::backup::BackupTrigger::Nightly;
                        // fix-180: read before `manifest` moves below, so a
                        // successful nightly backup refreshes the same
                        // owners an on-demand `Rpc::BackupStack` would.
                        let owners: Vec<String> =
                            homelab_core::ops::backup::owner_groups(&manifest)
                                .into_iter()
                                .map(|(owner, _paths)| owner)
                                .collect();
                        let r = run_op_locked(state, exec, 0, "scheduled-backup", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await
                            })
                        })
                        .await;
                        if r.ok {
                            kick_snapshot_refresh_many(state, state.config.backup.clone(), owners);
                        }
                        NightBackup::of(r.ok, r.deferred.as_deref())
                    }
                    // T5: several services share one container, so all of them are
                    // backed up and one failure fails the night for the stack —
                    // they share a container and a fate. Sequential WITHIN a
                    // stack: they are on the same container, so overlapping their
                    // pauses would stop that container twice over.
                    BackupWhat::Native(services) => {
                        let mut worst = NightBackup::Done;
                        for native in services {
                            let mut cfg = job.cfg.clone();
                            // fix-223: this is the nightly round — tag it as such.
                            cfg.trigger = homelab_core::ops::backup::BackupTrigger::Nightly;
                            let owner = native.unit.clone();
                            let r =
                                run_op_locked(state, exec, 0, "scheduled-backup-native", |ctx| {
                                    Box::pin(async move {
                                        homelab_core::ops::native::backup_native(ctx, &native, &cfg)
                                            .await
                                    })
                                })
                                .await;
                            if r.ok {
                                kick_snapshot_refresh(state, state.config.backup.clone(), owner);
                            }
                            worst = worst.worse_of(NightBackup::of(r.ok, r.deferred.as_deref()));
                        }
                        worst
                    }
                };
                if let NightBackup::Deferred(why) = &outcome {
                    info!("scheduler: backup for {} stood aside — {}", name, why);
                }
                BusyMark::step(state);
                (name, outcome)
            }
            .instrument(span)
        })
        .buffer_unordered(limit)
        .collect()
        .await;
    info!(
        "{}",
        homelab_core::ops::backup::phase_duration_line(
            phase_started.elapsed().as_secs(),
            stacks_in_phase,
            limit
        )
    );
    let end = unix_now();
    record_history(
        state,
        &homelab_core::history::HistoryEntry::Phase {
            start: end.saturating_sub(phase_started.elapsed().as_secs()),
            end,
            name: "backup".into(),
            count: stacks_in_phase,
        },
    );
    results.into_iter().collect()
}

/// One stack's share of the nightly backup phase.
pub(crate) struct BackupJob {
    pub(crate) stack: String,
    pub(crate) what: BackupWhat,
    pub(crate) cfg: homelab_core::ops::backup::BackupCfg,
}

pub(crate) enum BackupWhat {
    Compose(Box<homelab_proto::StackManifest>),
    Native(Vec<homelab_core::native::NativeServiceManifest>),
}

/// arch-history: append one entry to `<state_dir>/history.jsonl` (0600, one
/// write per line), and prune the file when it has grown past its size.
/// Best effort: history must never fail the operation it describes.
pub(crate) fn record_history(state: &AppState, entry: &homelab_core::history::HistoryEntry) {
    let path = format!("{}/history.jsonl", state.config.state_dir);
    if let Err(e) = append_audit(&path, &entry.to_line()) {
        tracing::warn!("history.jsonl :: {}", e);
        return;
    }
    let big = std::fs::metadata(&path)
        .map(|m| m.len() as usize > state.config.history_max_bytes)
        .unwrap_or(false);
    if big
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Some(kept) = homelab_core::history::prune(
            &text,
            unix_now(),
            state.config.history_max_age_s,
            state.config.history_max_bytes / 2,
        )
    {
        let tmp = format!("{}.tmp", path);
        if std::fs::write(&tmp, kept).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// A backup is a backup, whoever asked for it. The scheduler recorded
/// `last_backup` and the on-demand paths did not, so a stack backed up by
/// hand still read as never backed up — which is exactly what the fleet check
/// reported about kyu minutes after I had backed it up myself.
pub(crate) async fn record_backup_time(state: &AppState, stack: &str) {
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    record_state(&store, "last_backup", |s| {
        if let Some(rec) = s.stacks.get_mut(stack) {
            rec.last_backup = now;
        }
    })
    .await;
}
