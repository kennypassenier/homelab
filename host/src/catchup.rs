//! Catching a lagging session up, running beside the queue, the mirror push and parking after the night.

use crate::*;

/// fix-127: what a client that lagged past `n` messages is sent instead: a
/// warning that says so, then every question still waiting for an answer,
/// oldest first.
pub(crate) fn lag_catch_up(
    n: u64,
    asks: &std::sync::Mutex<std::collections::HashMap<u64, PendingAsk>>,
) -> Vec<ServerMsg> {
    let mut out = vec![ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        plan: None,
        level: homelab_proto::LogLevel::Warn,
        source: "HOST".into(),
        msg: format!(
            "{} message(s) to this client were dropped because it read too slowly; \
             open questions are sent again",
            n
        ),
    }];
    let guard = asks.lock().unwrap_or_else(PoisonError::into_inner);
    let mut open: Vec<(&u64, &PendingAsk)> = guard.iter().collect();
    open.sort_by_key(|(id, _)| **id);
    out.extend(open.into_iter().map(|(_, p)| p.ask.clone()));
    out
}

/// fix-66: requests that must not wait behind the one in flight. An answer
/// only hands a value to an operation that is parked waiting for it; queued
/// behind that same operation it can never arrive.
pub(crate) fn runs_beside_the_queue(command: &Rpc) -> bool {
    // A cancel is for the exec that holds the queue: behind it, it would
    // arrive when there is nothing left to stop.
    if matches!(command, Rpc::CancelExec { .. }) {
        return true;
    }
    // fix-68: `today` reads for about a minute (doctor plus the fleet check);
    // in the queue it would hold up every deploy and refresh the TUI sends
    // after opening. It changes nothing, and the TUI recognises its reply by
    // shape rather than by order.
    matches!(
        command,
        Rpc::Answer { .. } | Rpc::AnswerOpen { .. } | Rpc::Today { .. }
    )
}

/// D5: push the intent repo to the offsite mirror, detached — a failing
/// push logs and is retried after the next operation (and by the periodic
/// tick), never blocking the operation that triggered it.
pub(crate) fn spawn_mirror_push(state: &AppState) {
    let Some(remote) = state.config.mirror_remote.clone() else {
        return;
    };
    let repo = format!("{}/repo", state.config.state_dir);
    tokio::spawn(async move {
        if let Err(e) = homelab_core::ops::mirror::mirror_push(&RealExecutor, &repo, &remote).await
        {
            tracing::warn!("mirror push failed (will retry): {}", e);
        }
    });
}

/// H8: record what one stack's night parked, and say so once. The decision
/// is `ops::enable::after_night`; this is the load, save and notice around it.
pub(crate) async fn park_after_night(
    state: &AppState,
    exec: &RealExecutor,
    store: &homelab_core::state::StateStore<'_>,
    name: &str,
    update_ok: bool,
    now: u64,
) {
    let parked = record_state(store, "auto-park", |s| {
        homelab_core::ops::enable::after_night(s, name, update_ok, now)
    })
    .await
    .unwrap_or(false);
    if parked {
        tracing::warn!(
            "scheduler: nightly update for {} FAILED — automatic updates parked, backups continue; investigate, then resume with `homelab enable {}`",
            name,
            name
        );
    }
    if parked {
        notify_auto_disabled(
            state,
            exec,
            name,
            homelab_core::ops::enable::AUTO_PARK_NOTICE,
        )
        .await;
    }
}
