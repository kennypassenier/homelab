//! Process supervision: the systemd readiness/watchdog notification, the supervising loop that restarts a crashed task, and the terminate signal.

use crate::*;

/// B7: minimal sd_notify — tell systemd we're alive without pulling in a
/// crate. No-op when NOTIFY_SOCKET is unset (dev runs).
pub(crate) fn sd_notify(msg: &str) {
    if let Ok(sock) = std::env::var("NOTIFY_SOCKET") {
        let addr = if let Some(stripped) = sock.strip_prefix('@') {
            format!("\0{}", stripped)
        } else {
            sock
        };
        if let Ok(s) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = s.send_to(msg.as_bytes(), addr);
        }
    }
}

/// Run the daemon until something ends it, and say with which exit code.
///
/// fix-52 (see REGISTER.md): run the daemon so the scheduler and SIGTERM are
/// both supervised.
///
/// - The scheduler ending in any way is a failure (exit 1): systemd's
///   `Restart=always` brings the daemon back with a live scheduler.
/// - SIGTERM waits for the operation holding `op_lock`, at most `drain`,
///   then exits 0 (adoption norm N1). The lock is fair, so no operation
///   queued after the signal starts.
pub(crate) async fn supervise<S>(
    serve: S,
    scheduler: tokio::task::JoinHandle<()>,
    shutdown: impl std::future::Future<Output = ()>,
    op_lock: Arc<Mutex<()>>,
    drain: Duration,
) -> i32
where
    S: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        served = serve => {
            match served {
                Ok(()) => error!("server stopped without an error — exiting so systemd restarts it"),
                Err(e) => error!("server stopped :: {}", e),
            }
            1
        }
        ended = scheduler => {
            let how = match ended {
                Err(e) if e.is_panic() => "panicked",
                Err(_) => "was cancelled",
                Ok(()) => "returned",
            };
            error!(
                "scheduler task {} — exiting so systemd restarts the daemon with a live scheduler",
                how
            );
            1
        }
        () = shutdown => {
            info!(
                "SIGTERM: waiting up to {}s for the running operation to finish",
                drain.as_secs()
            );
            match tokio::time::timeout(drain, op_lock.lock_owned()).await {
                Ok(guard) => {
                    // Held until the process exits: nothing starts after this.
                    std::mem::forget(guard);
                    info!("SIGTERM: no operation running — exiting");
                }
                Err(_) => tracing::warn!(
                    "SIGTERM: an operation is still running after {}s — exiting anyway; the next start reports it as interrupted",
                    drain.as_secs()
                ),
            }
            0
        }
    }
}

/// fix-52: resolves on SIGTERM. If the handler cannot be installed the
/// daemon keeps running as it did before, and says so.
pub(crate) async fn terminate_signal() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            tracing::warn!("no SIGTERM handler ({}) — a stop kills the running step", e);
            std::future::pending::<()>().await;
        }
    }
}
