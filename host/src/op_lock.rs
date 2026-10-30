//! Running one operation at a time: the lock, the busy mark, progress lines and the wrappers that run a mutating operation.

use crate::*;

/// Run any mutating operation under the op-lock (AR12) with uniform incident
/// bundling on failure (AR14). The closure receives the OpCtx and returns the
/// OperationReport.
pub(crate) async fn run_mutating_op<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    run_mutating_op_for_stack(state, exec, req_id, label, None, op).await
}

/// Decision "deploys are known outages" (Kenny, 2026-09-30): the same as
/// [`run_mutating_op`], but names the one stack this operation acts on, so
/// `Rpc::Tiles` can say it is deploying rather than down while this runs.
pub(crate) async fn run_mutating_op_for_stack<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    stack: Option<&str>,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    let _guard = lock_ops(state).await;
    let _busy = BusyMark::set(state, label, 0, stack);
    let resp = run_op_locked(state, exec, req_id, label, op).await;
    // fix-stale-refresh-1: still under the operation lock, so nothing else
    // replaces the containers between the operation and the reading.
    if resp.ok
        && let Some(stack) = stack
        && refreshes_running_images(label)
    {
        refresh_running_images(state, exec, stack).await;
    }
    // pinned-digest-pruned-1: a deploy may have brought a new pin; keep it.
    if resp.ok && stack.is_some() && matches!(label, "deploy" | "update") {
        let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
        if let Ok(snapshot) = store.load().await {
            spawn_keep_round(state, &snapshot);
        }
    }
    resp
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// fix-104 (see REGISTER.md): take the operation lock, and when it is taken,
/// tell whoever is watching what this command waits for before waiting.
pub(crate) async fn lock_ops(state: &AppState) -> tokio::sync::MutexGuard<'_, ()> {
    if let Ok(g) = state.op_lock.try_lock() {
        return g;
    }
    let holder = state.busy.lock().ok().and_then(|b| b.clone());
    let msg = homelab_core::oplock::waiting_message(holder.as_ref(), unix_now());
    info!("{}", msg);
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        plan: None,
        level: homelab_proto::LogLevel::Warn,
        source: "HOST".into(),
        msg,
    });
    state.op_lock.lock().await
}

/// fix-104: one progress line of a long read (check, doctor, today) to
/// whoever is watching. Nobody connected, nothing sent.
pub(crate) fn progress_line(state: &AppState, line: &str) {
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        plan: None,
        level: homelab_proto::LogLevel::Info,
        source: "CHECK".into(),
        msg: line.to_string(),
    });
}

/// fix-104: records what holds the operation lock for as long as it lives.
pub(crate) struct BusyMark<'a> {
    pub(crate) state: &'a AppState,
}

impl<'a> BusyMark<'a> {
    pub(crate) fn set(state: &'a AppState, what: &str, total: usize, stack: Option<&str>) -> Self {
        if let Ok(mut b) = state.busy.lock() {
            *b = Some(homelab_core::oplock::Holder {
                what: what.to_string(),
                started_unix: unix_now(),
                done: 0,
                total,
                stack: stack.map(str::to_string),
            });
        }
        BusyMark { state }
    }

    /// One more item of a batch is finished.
    pub(crate) fn step(state: &AppState) {
        if let Ok(mut b) = state.busy.lock()
            && let Some(h) = b.as_mut()
        {
            h.done += 1;
        }
    }
}

impl Drop for BusyMark<'_> {
    fn drop(&mut self) {
        if let Ok(mut b) = self.state.busy.lock() {
            *b = None;
        }
    }
}

/// The body of a mutating operation WITHOUT taking the global lock.
///
/// Y1: the nightly backup phase holds that lock once for the whole round and
/// runs several backups inside it, so it needs the work without the locking.
/// Every RPC still goes through `run_mutating_op`, which is this plus the
/// lock — there is one implementation, not two that can drift.
pub(crate) async fn run_op_locked<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    // fix-122 (journal-lines-lack-op-context, 2026-09-27): every journal
    // line of the operation, its steps and its failure carries `op=<label>`;
    // the caller's span adds the stack.
    use tracing::Instrument as _;
    run_op_body(state, exec, req_id, label, op)
        .instrument(tracing::info_span!("op", op = %label))
        .await
}

pub(crate) async fn run_op_body<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    let broadcast = BroadcastSink {
        log_tx: state.log_tx.clone(),
        // Work the host starts itself runs with id 0: no request asked.
        req: (req_id != 0).then_some(req_id),
        recent: state.recent.clone(),
        recent_cap: state.config.recent_lines,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: requested_by(),
    };
    let sink = homelab_core::incidents::RecordingSink::new(&broadcast);
    let journal = FileJournal {
        path: format!("{}/journal.jsonl", state.config.state_dir),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let asker = LiveAsker {
        state,
        timeout_s: state.config.ask_timeout_s,
        notify: true,
    };
    // tile-watch-watcher-out (found 2026-10-01): the fleet-wide tile-watch
    // targets and the watcher stack itself, read once per op from the same
    // snapshot `deploy`'s own prior-state read uses — a deploy in progress
    // folds its own, fresher manifest in on top of this (`with_fresh_target`),
    // so a load a moment old is never wrong, only momentarily incomplete for
    // the very stack being deployed. A snapshot that fails to load leaves
    // both empty, same as `tile_watch_source` unset: no OUT rule is derived.
    let (tile_watch_targets, tile_watch_watcher) =
        match homelab_core::state::StateStore::new(exec, &state.config.state_dir)
            .load()
            .await
        {
            Ok(snapshot) => (
                homelab_core::ops::tiles::fleet_tile_watch_targets(&snapshot),
                state
                    .config
                    .tile_watch_source
                    .as_deref()
                    .and_then(|s| homelab_core::ops::tiles::tile_watch_watcher(&snapshot, s)),
            ),
            Err(_) => (Vec::new(), None),
        };
    let ctx = OpCtx {
        exec,
        sink: &sink,
        journal: &journal,
        safety: state.config.safety.clone(),
        state_dir: state.config.state_dir.clone(),
        now_unix: now,
        metrics_targets_dir: state.config.metrics_targets_dir.clone(),
        tile_watch_source: state.config.tile_watch_source.clone(),
        trusted_sources: state.config.trusted_sources.clone(),
        tile_watch_targets,
        tile_watch_watcher,
        // C1/C2: the same Loki the coverage check already asks about, so
        // there is not a second address to keep in step with the first.
        loki_url: state.config.loki_url.clone(),
        backup: state.config.backup.clone(),
        registry_cache: state.config.registry_cache.clone(),
        default_log_rotation: state.config.default_log_rotation.clone(),
        asker: &asker,
    };
    let report = op(&ctx).await;
    // arch-history: one line for this operation, whatever its outcome.
    record_history(
        state,
        &homelab_core::history::HistoryEntry::Op {
            start: now,
            end: unix_now(),
            label: label.to_string(),
            subject: broadcast.subject.lock().ok().and_then(|s| s.clone()),
            req: (req_id != 0).then_some(req_id),
            by: broadcast.by.clone(),
            ok: report.ok,
            deferred: report.deferred.clone(),
            error: report.error.as_ref().map(|e| e.what.clone()),
            steps: broadcast
                .timings
                .lock()
                .map(|t| t.clone())
                .unwrap_or_default(),
        },
    );
    // AR14: a failure's incident bundle first, so its notice can name it.
    let bundle = if !report.ok && report.deferred.is_none() {
        let versions = format!("host={}\nproto={}\n", VERSION, homelab_proto::PROTO_VERSION);
        Some(
            homelab_core::incidents::write_bundle(
                exec,
                &state.config.state_dir,
                now,
                &report,
                &sink.events(),
                &versions,
            )
            .await,
        )
    } else {
        None
    };
    let incident = match &bundle {
        Some(Ok(dir)) => dir.rsplit('/').next().map(str::to_string),
        _ => None,
    };
    // F3, best-effort; decision notify-routing: a notice, pushed if urgent.
    notify(
        state,
        exec,
        label,
        &report,
        now,
        (req_id != 0).then_some(req_id),
        incident,
    )
    .await;
    // rule-20: this operation's own leftovers, success or not — a push that
    // failed partway is exactly the case `push_content_staged`'s own cleanup
    // never reaches (that `rm` is only on its success path).
    cleanup_push_staging(&state.config.state_dir, now);
    if report.ok {
        spawn_mirror_push(state); // D5, best-effort + detached
    }
    if report.ok {
        RpcResponse {
            id: req_id,
            ok: true,
            // fix-371-1: a run that did nothing says why, in one line.
            message: match &report.note {
                Some(note) => format!("{} complete — nothing to do: {}", label, note),
                None => format!(
                    "{} complete — {} step(s), {} changed",
                    label,
                    report.steps.len(),
                    report.steps.iter().filter(|s| s.changed).count()
                ),
            },
            deferred: None,
        }
    } else if let Some(why) = report.deferred.clone() {
        // Stood aside on purpose. No incident bundle, no `error!`, no
        // failure count: nothing broke and nothing changed. `ok` stays false
        // because nothing ran either, so no caller records work that did not
        // happen (F280).
        info!("{} stood aside: {}", label, why);
        RpcResponse {
            id: req_id,
            ok: false,
            message: format!("{} deferred — {}", label, why),
            deferred: Some(why),
        }
    } else {
        let err = report
            .error
            .clone()
            .unwrap_or(homelab_core::error::OperatorError {
                what: format!("{} failed", label),
                why: "unknown".into(),
                remedy: "see transcript".into(),
            });
        error!("{} failed: {} — {}", label, err.what, err.why);
        let bundle_note = match bundle {
            Some(Ok(dir)) => format!(" :: incident bundle {}", dir),
            Some(Err(e)) => format!(" :: (bundle write failed: {})", e),
            None => String::new(),
        };
        RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "{} :: {} :: remedy: {}{}",
                err.what, err.why, err.remedy, bundle_note
            ),
            deferred: None,
        }
    }
}
