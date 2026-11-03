//! Notices for the dashboard: publishing and recording one (in the host database), and the audit file helpers.

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

/// The next notice sequence number (grows across restarts).
pub(crate) fn next_notice_seq(state: &AppState) -> u64 {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut last = state
        .notice_seq
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    *last = homelab_core::notify::next_seq(*last, now_ms);
    *last
}

/// The one push to kyu for an urgent notice: the payload, the dedup key
/// (kyu drops a repeated failure for 20 h), and what became of it.
async fn push_urgent(
    state: &AppState,
    exec: &RealExecutor,
    f: &NoticeFacts,
) -> Result<String, String> {
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
    let dedup = homelab_core::notify_kyu::dedup_key(&f.op, f.ok, Some(&f.ex.what));
    notify_raw(state, exec, payload, dedup).await
}

/// Decision notify-routing (Kenny, 2026-09-30): every event becomes a notice
/// the dashboard reads (`Command::Notices`); only an urgent one is also
/// pushed at once, to kyu (`notify_webhook`), with the short text and the
/// link to the dashboard page. Decision kyu-notify ("Eenvoudig"): that is
/// ONE POST; kyu damps repeats (`Kyu-Dedup-Key`) and owns retries and the
/// fallback to Home Assistant. When kyu does not accept it, a second notice
/// "Notification could not be delivered" is written beside the first.
/// Returns whether kyu accepted the push, as opposed to "centre only" or a
/// failure — fix-184's `watch_dashboard` uses this to decide whether a
/// later recovery is itself allowed to push.
pub(crate) async fn publish_notice(state: &AppState, exec: &RealExecutor, f: NoticeFacts) -> bool {
    let now = unix_now();
    let mut failure = None;
    let route = notify_route(state);
    let push = if !f.urgency.urgent {
        "centre only".to_string()
    } else if route.is_none() {
        // An unconfigured host stays quiet: nothing was attempted, so
        // nothing "could not be delivered".
        "not pushed: no notification route is configured (notify_webhook)".to_string()
    } else {
        match push_urgent(state, exec, &f).await {
            Ok(text) => text,
            Err(why) => {
                let text = format!("failed: {}", why);
                failure = Some(why);
                text
            }
        }
    };
    let pushed_out = f.urgency.urgent && route.is_some() && failure.is_none();
    let seq = next_notice_seq(state);
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
    if let (Some(why), Some(route)) = (failure, route) {
        let lost = homelab_core::notify_kyu::undelivered_notice(
            next_notice_seq(state),
            now,
            &notice,
            &why,
            &homelab_core::notify::route_for_log(&route),
        );
        record_undelivered(state, lost, &why);
    }
    pushed_out
}

/// The configured push route, if any.
fn notify_route(state: &AppState) -> Option<String> {
    state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .notify_webhook
        .clone()
}

/// Write an undelivered notice, collapsing it into the one already open for
/// the same destination (count and last-seen move, no second row), so kyu
/// being down cannot grow the table by one notice per push.
fn record_undelivered(state: &AppState, lost: homelab_core::notify::HostNotice, why: &str) {
    let dir = &state.config.state_dir;
    let all = match hostdb::for_dir(dir).and_then(|db| hostdb::notices::all(&db)) {
        Ok(all) => all,
        Err(e) => return tracing::warn!("notices: could not read the open ones :: {e}"),
    };
    let route = lost.label.strip_prefix("notification:").unwrap_or("");
    let Some(prev) = all
        .iter()
        .rev()
        .find(|n| homelab_core::notify_kyu::is_undelivered_for(n, route))
    else {
        record_notice(state, &lost);
        return;
    };
    let latest = lost.what.split('"').nth(1).unwrap_or("").to_string();
    let folded = homelab_core::notify_kyu::fold_undelivered(prev, lost, why, &latest);
    let swapped =
        hostdb::for_dir(dir).and_then(|db| hostdb::notices::replace(&db, prev.seq, &folded));
    if let Err(e) = swapped {
        tracing::warn!("notices: could not fold the undelivered notice :: {e}");
    }
}

/// Add one notice and cut the table by the history's limits. Best effort,
/// like the history: a notice must never fail its operation, but a loss is
/// logged.
pub(crate) fn record_notice(state: &AppState, n: &homelab_core::notify::HostNotice) {
    let written = hostdb::for_dir(&state.config.state_dir).and_then(|db| {
        hostdb::notices::append(
            &db,
            n,
            unix_now(),
            state.config.history_max_age_s,
            state.config.history_max_bytes,
        )
    });
    if let Err(e) = written {
        tracing::warn!("notices: could not record a notice :: {e}");
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
