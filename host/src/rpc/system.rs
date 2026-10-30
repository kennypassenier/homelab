//! RPC handlers: system.

use crate::*;
use homelab_proto::*;

/// bypassed it (a test harness), which changes nothing.
/// arch-history: what the host did since a moment.
pub(crate) async fn history(
    state: &AppState,
    req_id: u64,
    since: u64,
    until: Option<u64>,
    limit: usize,
) -> RpcResponse {
    let text = std::fs::read_to_string(format!("{}/history.jsonl", state.config.state_dir))
        .unwrap_or_default();
    let entries = homelab_core::history::select_between(
        homelab_core::history::parse(&text),
        since,
        until,
        limit,
    );
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({ "entries": entries }).to_string(),
        deferred: None,
    }
}

/// Decision notify-routing: the notices after the dashboard's cursor.
pub(crate) async fn notices(
    state: &AppState,
    req_id: u64,
    after: u64,
    limit: usize,
) -> RpcResponse {
    let text = std::fs::read_to_string(notices_path(&state.config.state_dir)).unwrap_or_default();
    let all = homelab_core::notify::parse_notices(&text);
    let last_seq = all.iter().map(|n| n.seq).max().unwrap_or(0);
    let notices = homelab_core::notify::notices_after(all, after, limit.min(1000));
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({ "notices": notices, "last_seq": last_seq }).to_string(),
        deferred: None,
    }
}

/// feat-platform-3: what runs now, and the newest lines.
pub(crate) async fn current_op(state: &AppState, req_id: u64) -> RpcResponse {
    let holder = state.busy.lock().ok().and_then(|b| b.clone());
    let view = homelab_proto::CurrentOpView {
        holder: holder.as_ref().map(|h| h.what.clone()),
        started_unix: holder.as_ref().map(|h| h.started_unix),
        lines: state
            .recent
            .lock()
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default(),
    };
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::to_string(&view).unwrap_or_default(),
        deferred: None,
    }
}

/// fix-68 (see REGISTER.md): hands back the same fleet snapshot
/// `homelab ui`/the dashboard use, which the client turns
/// into a short human table, or prints as-is for `--json`.
pub(crate) async fn status(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    let fleet = build_fleet_state(state, &exec).await;
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::to_string(&fleet).unwrap_or_else(|_| "{}".into()),
        deferred: None,
    }
}

/// `SelfUpdateHost`.
pub(crate) async fn self_update_host(
    state: &AppState,
    req_id: u64,
    binary_b64: String,
    proof: Option<ReleaseProof>,
) -> RpcResponse {
    let exec = RealExecutor;
    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&binary_b64) {
        Ok(b) => b,
        Err(e) => {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: format!("bad binary payload: {}", e),
                deferred: None,
            };
        }
    };
    // redesign-host-4: a release's signature is verified here too
    // and recorded, so the new daemon can report itself a signed
    // release. One that does not verify is refused, never recorded;
    // a binary shipped without one installs as before and reports
    // itself unsigned.
    if let Some(p) = &proof
        && let Err(e) = homelab_core::release_sig::record_proof(
            std::path::Path::new(&state.config.state_dir),
            SELF_ASSET,
            &bytes,
            &p.sums,
            &p.sig,
        )
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("refused: the release signature sent with the binary: {e}"),
            deferred: None,
        };
    }
    let cfg = homelab_core::ops::selfupdate::SelfUpdateCfg::default();
    // Stage outside the op so the (large) write is done before the
    // op-lock is taken. Raw bytes, not write_file (which is text).
    if let Err(e) = std::fs::write(&cfg.staged, &bytes).and_then(|()| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cfg.staged, std::fs::Permissions::from_mode(0o755))
    }) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("stage binary: {}", e),
            deferred: None,
        };
    }
    info!("self-update requested: staged {} bytes", bytes.len());
    run_mutating_op(state, &exec, req_id, "self-update", |ctx| {
        Box::pin(async move { homelab_core::ops::selfupdate::self_update(ctx, &cfg).await })
    })
    .await
}

/// Owner decision 2026-09-30 (item 2): "Save and restart the host".
/// Refused outright rather than queued: waiting behind a running
/// job and then restarting mid-nightly-round is not what "restart
/// the host" should ever mean.
pub(crate) async fn restart_host(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    if let Some(holder) = state.busy.lock().ok().and_then(|b| b.clone()) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "refused: an operation is running ({}); running jobs are refused while a job runs",
                holder.what
            ),
            deferred: None,
        };
    }
    run_mutating_op(state, &exec, req_id, "restart-host", |ctx| {
        Box::pin(async move { homelab_core::ops::restarthost::restart_host(ctx).await })
    })
    .await
}

/// `Doctor`.
pub(crate) async fn doctor(state: &AppState, req_id: u64, json: bool) -> RpcResponse {
    let exec = RealExecutor;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut probes = gather_probes(
        &exec,
        &state.config.state_dir,
        state.config.mirror_remote.as_deref(),
        state.config.backup.staging_dir.as_deref(),
        &state.config.restore_drill_scratch_dir,
        now,
        &state.snapshot_cache,
        &state.config.backup,
        &|line: &str| progress_line(state, line),
    )
    .await;
    probes.failed_auth = Some(state.auth_failures.snapshot());
    probes.tools = super::tools::tool_probes(state).await;
    gather_security_probes(&exec, &ProbeContext::of(&state.config), now, &mut probes).await;
    let checks = homelab_core::doctor::diagnose(&probes);
    let overall = homelab_core::doctor::overall(&checks);
    if json {
        return RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::json!({ "overall": overall, "checks": checks }).to_string(),
            deferred: None,
        };
    }
    let mut msg = format!("doctor: {:?}\n", overall);
    for c in &checks {
        msg.push_str(&format!("  [{:?}] {} — {}\n", c.health, c.name, c.detail));
        if let Some(r) = &c.remedy {
            msg.push_str(&format!("        ↳ {}\n", r));
        }
    }
    RpcResponse {
        id: req_id,
        ok: overall != homelab_core::doctor::Health::Fail,
        message: msg,
        deferred: None,
    }
}

/// `GetState`.
pub(crate) async fn get_state(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    let fleet = build_fleet_state(state, &exec).await;
    let _ = state.log_tx.send(ServerMsg::State(Box::new(fleet)));
    RpcResponse {
        id: req_id,
        ok: true,
        message: "state".into(),
        deferred: None,
    }
}

/// fix-131: one bundle, read on the workstation.
pub(crate) async fn incident_show(state: &AppState, req_id: u64, name: String) -> RpcResponse {
    let (ok, message) = match incident_text(&state.config.state_dir, &name) {
        Ok(text) => (true, text),
        Err(why) => (false, why),
    };
    RpcResponse {
        id: req_id,
        ok,
        message,
        deferred: None,
    }
}

/// `Incidents`.
pub(crate) async fn incidents(state: &AppState, req_id: u64, json: bool) -> RpcResponse {
    let dir = format!("{}/incidents", state.config.state_dir);
    let list = std::fs::read_dir(&dir)
        .map(|rd| {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    RpcResponse {
        id: req_id,
        ok: true,
        message: if json {
            serde_json::json!({ "incidents": list }).to_string()
        } else if list.is_empty() {
            "no incidents recorded".into()
        } else {
            format!("incidents:\n  {}", list.join("\n  "))
        },
        deferred: None,
    }
}
