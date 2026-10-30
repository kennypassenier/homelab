//! RPC handlers: config.

use crate::*;
use homelab_proto::*;

/// `GetConfig`.
pub(crate) async fn get_config(state: &AppState, req_id: u64) -> RpcResponse {
    let view = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let _ = state.log_tx.send(ServerMsg::Config(Box::new(view)));
    RpcResponse {
        id: req_id,
        ok: true,
        message: "config".into(),
        deferred: None,
    }
}

/// `SetConfig`.
pub(crate) async fn set_config(
    state: &AppState,
    req_id: u64,
    view: Box<HostConfigView>,
) -> RpcResponse {
    // Validate before persisting.
    if let Some(h) = view.backup_hour
        && h > 23
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: "backup_hour must be 0-23".into(),
            deferred: None,
        };
    }
    if view.retention.is_empty() {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: "retention needs at least one tier".into(),
            deferred: None,
        };
    }
    match persist_settings(&state.config, &view) {
        Ok(()) => {
            *state
                .settings
                .write()
                .unwrap_or_else(PoisonError::into_inner) = *view;
            info!("settings updated by a client");
            RpcResponse {
                id: req_id,
                ok: true,
                message: "settings saved and applied".into(),
                deferred: None,
            }
        }
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("persist settings: {}", e),
            deferred: None,
        },
    }
}

/// feat-settings-1: read and write host.toml for the dashboard. The
/// answers go to the asking session only; nothing is broadcast, so a
/// TUI's settings screen keeps its unsaved edits.
pub(crate) async fn get_host_config(state: &AppState, req_id: u64) -> RpcResponse {
    let path = &state.config.config_path;
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    match host_config_view(path, &raw) {
        Ok(view) => RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::to_string(&view).unwrap_or_default(),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
    }
}

/// `SetHostConfig`.
pub(crate) async fn set_host_config(
    state: &AppState,
    req_id: u64,
    changes: std::collections::BTreeMap<String, serde_json::Value>,
    expect_sha256: String,
) -> RpcResponse {
    // fix-false-alarm (2026-09-30): a `watch_url` the host cannot
    // reach would not fail until the notice five minutes later —
    // the same request `watch_dashboard` runs every minute, once,
    // before the value is even written.
    if let Some(url) = watch_url_to_check(&changes) {
        let out = RealExecutor
            .run(&homelab_core::executor::Cmd::new(
                "curl",
                &["-sf", "-m", "10", "-o", "/dev/null", &url],
                20,
            ))
            .await;
        let reached = out.as_ref().map(|o| o.success()).unwrap_or(false);
        if let Some(reason) = watch_url_refusal(&url, reached) {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: reason,
                deferred: None,
            };
        }
    }
    let path = state.config.config_path.clone();
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let outcome = apply_host_config_changes(&raw, &changes, &expect_sha256)
        .and_then(|(text, saved)| write_config_file(&path, &text).map(|()| (text, saved)));
    match outcome {
        Ok((text, saved)) => {
            // The G8 keys are live: the scheduler hour, the webhook,
            // the retention and (fix-122) the log level read the
            // settings, not the file.
            if let Ok(file) = toml::from_str::<FileConfig>(&text) {
                let new_level = file
                    .log_level
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "info".to_string());
                {
                    let mut live = state
                        .settings
                        .write()
                        .unwrap_or_else(PoisonError::into_inner);
                    live.backup_hour = file.backup_hour;
                    live.notify_webhook = file.notify_webhook;
                    live.retention = file
                        .retention
                        .unwrap_or_else(homelab_core::retention::default_tiers);
                    live.log_level = new_level.clone();
                }
                // fix-122: reload the filter both the journald sink
                // and the JSONL ring read from. `startup_problems`
                // already refused an unparsable directive before the
                // file was written, so this falls back to "info"
                // only in the theoretical case of a file edited by
                // hand between the write above and this line.
                let _ = state.log_filter.reload(
                    tracing_subscriber::EnvFilter::try_new(&new_level)
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                );
            }
            info!(
                "host.toml changed via the dashboard: {}",
                changes.keys().cloned().collect::<Vec<_>>().join(", ")
            );
            RpcResponse {
                id: req_id,
                ok: true,
                message: serde_json::to_string(&saved).unwrap_or_default(),
                deferred: None,
            }
        }
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
    }
}
