//! Notices for the dashboard: where they are kept, publishing and recording one, and the audit file helpers.

use crate::*;

/// Decision notify-routing (2026-09-30): what one notice is made of.
pub(crate) struct NoticeFacts {
    pub(crate) op: String,
    pub(crate) label: String,
    pub(crate) ok: bool,
    pub(crate) deferred: bool,
    /// Since when it is so (the operation's start).
    pub(crate) since: u64,
    pub(crate) ex: homelab_core::notify::Explained,
    pub(crate) urgency: homelab_core::notify::Urgency,
    pub(crate) incident: Option<String>,
    pub(crate) req: Option<u64>,
    pub(crate) by: Option<String>,
    /// The nightly check's findings (empty for everything else).
    pub(crate) findings: Vec<homelab_core::ops::fleetcheck::Finding>,
}

/// Where the host keeps its notices for the dashboard (0600, one line each).
pub(crate) fn notices_path(state_dir: &str) -> String {
    format!("{}/notices.jsonl", state_dir)
}

/// Decision notify-routing (Kenny, 2026-09-30): every event becomes a notice
/// the dashboard reads (`Command::Notices`); only an urgent one is also
/// pushed at once, through kyu (fallback: Home Assistant) as before, with
/// the short text and the link to the dashboard page. The damper (H13)
/// still keeps one failure from paging every night; it now judges pushes
/// only, so the centre keeps every occurrence.
/// Returns whether this notice actually left the centre as an urgent push
/// (`notify_webhook`), as opposed to staying "centre only" or being
/// swallowed by the 20 h damper — fix-184's `watch_dashboard` uses this to
/// decide whether a later recovery is itself allowed to push.
pub(crate) async fn publish_notice(state: &AppState, exec: &RealExecutor, f: NoticeFacts) -> bool {
    let now = unix_now();
    let mut pushed_out = false;
    let push = if !f.urgency.urgent {
        "centre only".to_string()
    } else if !state
        .damper
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .should_send(&f.op, f.ok, Some(&f.ex.what), now)
    {
        "not pushed again: the same failure went out within 20 h".to_string()
    } else {
        pushed_out = true;
        let url = homelab_core::notify::click_url(&state.config.dashboard_url, &f.ex.page);
        let short = homelab_core::notify::push_short(&f.ex.title, &f.ex.remedy);
        let payload = homelab_core::notify::push_payload(
            "homelab-host",
            &f.op,
            &f.label,
            f.ok,
            Some(&short),
            VERSION,
            Some(url.as_str()).filter(|u| !u.is_empty()),
        );
        match notify_raw(state, exec, payload).await {
            Ok(()) => "sent".to_string(),
            Err(why) => format!("failed: {}", why),
        }
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = {
        let mut last = state
            .notice_seq
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *last = homelab_core::notify::next_seq(*last, now_ms);
        *last
    };
    let notice = homelab_core::notify::HostNotice {
        seq,
        at: now,
        since: f.since,
        op: f.op,
        label: f.label,
        ok: f.ok,
        deferred: f.deferred,
        stack: f.ex.stack,
        title: f.ex.title,
        what: f.ex.what,
        consequence: f.ex.consequence,
        remedy: f.ex.remedy,
        page: f.ex.page,
        urgent: f.urgency.urgent,
        routed: f.urgency.why.to_string(),
        push,
        incident: f.incident,
        req: f.req,
        by: f.by,
        findings: f.findings,
    };
    record_notice(state, &notice);
    pushed_out
}

/// Append one notice (0600) and prune the file by the history's limits.
/// Best effort, like the history: a notice must never fail its operation.
pub(crate) fn record_notice(state: &AppState, n: &homelab_core::notify::HostNotice) {
    let path = notices_path(&state.config.state_dir);
    let line = match serde_json::to_string(n) {
        Ok(l) => l + "\n",
        Err(e) => {
            tracing::warn!("notices.jsonl :: {}", e);
            return;
        }
    };
    if let Err(e) = append_audit(&path, &line) {
        tracing::warn!("notices.jsonl :: {}", e);
        return;
    }
    let big = std::fs::metadata(&path)
        .map(|m| m.len() as usize > state.config.history_max_bytes)
        .unwrap_or(false);
    if big
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Some(kept) = homelab_core::notify::prune_notices(
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

/// F3: a notice after every mutating operation (see [`publish_notice`]).
/// Never blocks or fails the operation itself.
pub(crate) async fn notify(
    state: &AppState,
    exec: &RealExecutor,
    label: &str,
    report: &homelab_core::runner::OperationReport,
    since: u64,
    req: Option<u64>,
    incident: Option<String>,
) {
    let ex = homelab_core::notify::explain_op(&homelab_core::notify::OpFacts {
        op: &report.op,
        label,
        ok: report.ok,
        deferred: report.deferred.as_deref(),
        error: report.error.as_ref(),
        incident: incident.as_deref(),
    });
    let urgency = homelab_core::notify::urgency(&homelab_core::notify::Event::Op {
        label,
        ok: report.ok,
        deferred: report.deferred.is_some(),
    });
    publish_notice(
        state,
        exec,
        NoticeFacts {
            op: report.op.clone(),
            label: label.to_string(),
            ok: report.ok,
            deferred: report.deferred.is_some(),
            since,
            ex,
            urgency,
            incident,
            req,
            by: requested_by(),
            findings: Vec::new(),
        },
    )
    .await;
}

/// fix-51 (see REGISTER.md): every short read-modify-write of state.json
/// outside an operation goes through the store's lock, and a failure is
/// logged instead of dropped.
pub(crate) async fn record_state<R>(
    store: &homelab_core::state::StateStore<'_>,
    what: &str,
    change: impl FnOnce(&mut homelab_core::state::HostState) -> R,
) -> Option<R> {
    match store.update(change).await {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!("state: could not record {} :: {}", what, e);
            None
        }
    }
}

/// Keep the last word on whether notifications are arriving, so a broken
/// notification path is visible somewhere other than in a notification.
///
/// That circularity is real and worth stating: if every route is down, this
/// record is what `homelab check` and the TUI read, because the report saying
/// so cannot reach him by the path that is broken.
pub(crate) async fn record_notify_outcome(
    state: &AppState,
    exec: &RealExecutor,
    delivered: bool,
    why: &str,
) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    record_state(&store, "notification outcome", |st| {
        if delivered {
            st.last_notify_ok = now;
            st.last_notify_error = None;
        } else {
            st.last_notify_failed = now;
            st.last_notify_error = Some(why.to_string());
        }
    })
    .await;
}
