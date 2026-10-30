//! RPC handlers: running one command in a container and cancelling it.

use crate::*;

/// `ExecIn`.
pub(crate) async fn exec_in(
    state: &AppState,
    req_id: u64,
    vmid: u16,
    command: String,
) -> RpcResponse {
    let exec = RealExecutor;
    if let Err(e) = exec_allowed(&state.config, vmid) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("{}", e),
            deferred: None,
        };
    }
    // A6: audit every invocation before running it.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let audit = exec_audit_line(ts, vmid, &command);
    let audit_path = format!("{}/audit.log", state.config.state_dir);
    if let Err(e) = append_audit(&audit_path, &audit) {
        tracing::warn!("audit.log: could not record the exec :: {}", e);
    }
    info!("audit: {}", audit.trim_end());
    let key = (requested_by().unwrap_or_default(), req_id);
    let tag = format!("homelab-exec-{}-{}", req_id, ts);
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    RUNNING_EXECS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            key.clone(),
            RunningExec {
                vmid,
                tag: tag.clone(),
                cancelled: cancelled.clone(),
            },
        );
    let _running = RunningExecGuard(key);
    let ran = homelab_core::executor::pct_sh_tagged(&exec, vmid, &command, 120, Some(&tag)).await;
    let stopped = cancelled.load(std::sync::atomic::Ordering::SeqCst);
    match ran {
        Ok(out) => RpcResponse {
            id: req_id,
            ok: out.success() && !stopped,
            message: format!(
                "{}exit {}\n{}{}",
                if stopped { "cancelled\n" } else { "" },
                out.code,
                out.stdout,
                if out.stderr.is_empty() {
                    String::new()
                } else {
                    format!("--- stderr ---\n{}", out.stderr)
                }
            ),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("{}{}", if stopped { "cancelled\n" } else { "" }, e),
            deferred: None,
        },
    }
}

/// Stop the exec a request id names, cleanly: TERM inside the
/// container, KILL after the grace `timeout -k` holds. The exec's own
/// reply then starts `cancelled`.
pub(crate) async fn cancel_exec(state: &AppState, req_id: u64, request: u64) -> RpcResponse {
    let exec = RealExecutor;
    let by = requested_by().unwrap_or_default();
    let found = RUNNING_EXECS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(by, request))
        .map(|r| (r.vmid, r.tag.clone(), r.cancelled.clone()));
    let Some((vmid, tag, cancelled)) = found else {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "no command is running for request {request}; it may have ended already"
            ),
            deferred: None,
        };
    };
    let ts = unix_now();
    let audit = format!("{ts} exec-cancel vmid={vmid} request={request}\n");
    let audit_path = format!("{}/audit.log", state.config.state_dir);
    if let Err(e) = append_audit(&audit_path, &audit) {
        tracing::warn!("audit.log: could not record the cancel :: {}", e);
    }
    info!("audit: {}", audit.trim_end());
    cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    let stop = homelab_core::executor::stop_tagged_script(&tag);
    match homelab_core::executor::pct_sh(&exec, vmid, &stop, 15).await {
        Ok(out) if out.success() => RpcResponse {
            id: req_id,
            ok: true,
            message: format!("stop signal sent to the command in CT {vmid}"),
            deferred: None,
        },
        Ok(out) => {
            cancelled.store(false, std::sync::atomic::Ordering::SeqCst);
            RpcResponse {
                id: req_id,
                ok: false,
                message: format!(
                    "the command in CT {vmid} could not be signalled (exit {}); it may have ended already",
                    out.code
                ),
                deferred: None,
            }
        }
        Err(e) => {
            cancelled.store(false, std::sync::atomic::Ordering::SeqCst);
            RpcResponse {
                id: req_id,
                ok: false,
                message: format!("{e}"),
                deferred: None,
            }
        }
    }
}
