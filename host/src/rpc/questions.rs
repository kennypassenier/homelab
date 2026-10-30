//! RPC handlers: questions.

use crate::*;

/// T69: the operator answered a suspended step. Delivering it is all
/// that happens here — the step itself is parked on a channel inside
/// the operation, not on this task.
pub(crate) async fn answer(
    state: &AppState,
    req_id: u64,
    id: u64,
    allow: bool,
    boot: Option<String>,
) -> RpcResponse {
    // arch-host-link: an answer to a question from an earlier start of
    // this host must not answer today's question with the same id.
    if boot.as_ref().is_some_and(|b| *b != state.boot_id) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "question {} was asked by an earlier start of the host — not delivered",
                id
            ),
            deferred: None,
        };
    }
    let delivered = state
        .pending_asks
        .lock()
        .ok()
        .and_then(|mut g| g.remove(&id))
        .map(|p| p.reply.send(allow).is_ok())
        .unwrap_or(false);
    RpcResponse {
        id: req_id,
        ok: delivered,
        message: if delivered {
            format!("answer delivered to question {}", id)
        } else {
            // Not an error worth an incident: a question times out on
            // its own, so an answer arriving late is ordinary.
            format!(
                "question {} is no longer waiting — it timed out or was answered",
                id
            )
        },
        deferred: None,
    }
}

/// fix-240: answer a running operation's question from ANY session —
/// a headless run cannot answer its own, a second client or Live view
/// can. The same delivery as `Answer`, found by operation rather than
/// by an id only the asking session saw.
pub(crate) async fn answer_open(
    state: &AppState,
    req_id: u64,
    op: Option<String>,
    allow: bool,
) -> RpcResponse {
    let open = open_questions(state);
    let picked = homelab_core::ask::pick_open(&open, op.as_deref()).cloned();
    let (ok, message) = match picked {
        Err(why) => (false, why),
        Ok(q) => {
            let delivered = state
                .pending_asks
                .lock()
                .ok()
                .and_then(|mut g| g.remove(&q.id))
                .map(|p| p.reply.send(allow).is_ok())
                .unwrap_or(false);
            if delivered {
                (
                    true,
                    format!(
                        "{} delivered to {} :: {} (question {})",
                        if allow { "allow" } else { "stop" },
                        q.op,
                        q.step,
                        q.id
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "question {} is no longer waiting — it timed out or was answered",
                        q.id
                    ),
                )
            }
        }
    };
    RpcResponse {
        id: req_id,
        ok,
        message,
        deferred: None,
    }
}

/// `ListManualChecks`.
pub(crate) async fn list_manual_checks(state: &AppState, req_id: u64, json: bool) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let st = store.load().await.unwrap_or_default();
    let rows = homelab_core::ops::manualchecks::listing(&st);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    RpcResponse {
        id: req_id,
        ok: true,
        message: if json {
            serde_json::json!({
                "now": now,
                "checks": rows
                    .iter()
                    .map(|(id, rec)| {
                        // fix-195 round 2: the report's own rule, so
                        // the dashboard cannot show "ok" for an
                        // answer the report has reopened.
                        let reopened =
                            homelab_core::ops::manualchecks::open_again(&st, rec, now);
                        serde_json::json!({ "id": id, "record": rec, "reopened": reopened })
                    })
                    .collect::<Vec<_>>(),
            })
            .to_string()
        } else {
            homelab_core::ops::manualchecks::render_listing(&st, now)
        },
        deferred: None,
    }
}

/// fix-65: record a person's answer to one manual check, or accept a not-ok
/// until a date.
pub(crate) async fn answer_manual_check(
    state: &AppState,
    req_id: u64,
    id: String,
    ok: bool,
    note: String,
    accept_days: Option<u32>,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // fix-65: a deliberate nok, accepted until a date.
    let until = accept_days.map(|d| now + u64::from(d) * 86_400);
    let found = record_state(&store, "manual check answer", |st| match until {
        Some(u) => homelab_core::ops::manualchecks::accept(st, &id, u, &note, now),
        None => homelab_core::ops::manualchecks::answer(st, &id, ok, &note, now),
    })
    .await
    .unwrap_or(false);
    let message = if found {
        match until {
            Some(u) => format!(
                "{} recorded as NOT ok, accepted until {}",
                id,
                homelab_core::state::ymd(u)
            ),
            None => format!("{} recorded as {}", id, if ok { "ok" } else { "NOT ok" }),
        }
    } else {
        format!(
            "no manual check has id {} — run `homelab checks` for the list",
            id
        )
    };
    RpcResponse {
        id: req_id,
        ok: found,
        message,
        deferred: None,
    }
}
