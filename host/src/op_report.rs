//! What an operation leaves behind once it has run: its history line, the
//! incident bundle of a failure, and the response the caller gets.

use crate::*;

/// arch-history: one line for this operation, whatever its outcome.
pub(crate) fn record_op_history(
    state: &AppState,
    broadcast: &BroadcastSink,
    start: u64,
    label: &str,
    req_id: u64,
    report: &homelab_core::runner::OperationReport,
) {
    record_history(
        state,
        &homelab_core::history::HistoryEntry::Op {
            start,
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
}

/// AR14: a failure's incident bundle, written first so its notice can name
/// it. A success or a deferral leaves none.
pub(crate) async fn failure_bundle(
    state: &AppState,
    exec: &RealExecutor,
    now: u64,
    report: &homelab_core::runner::OperationReport,
    events: &[homelab_core::sink::PipelineEvent],
) -> Option<Result<String, homelab_core::error::CoreError>> {
    if report.ok || report.deferred.is_some() {
        return None;
    }
    let versions = format!("host={}\nproto={}\n", VERSION, homelab_proto::PROTO_VERSION);
    Some(
        homelab_core::incidents::write_bundle(
            exec,
            &state.config.state_dir,
            now,
            report,
            events,
            &versions,
        )
        .await,
    )
}

/// The bundle's directory name, which the notice names.
pub(crate) fn bundle_name(
    bundle: &Option<Result<String, homelab_core::error::CoreError>>,
) -> Option<String> {
    match bundle {
        Some(Ok(dir)) => dir.rsplit('/').next().map(str::to_string),
        _ => None,
    }
}

/// The caller's answer: complete, stood aside on purpose, or failed.
pub(crate) fn op_response(
    label: &str,
    req_id: u64,
    report: &homelab_core::runner::OperationReport,
    bundle: Option<Result<String, homelab_core::error::CoreError>>,
) -> RpcResponse {
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
