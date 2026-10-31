//! Starting the daemon: the argument check, the housekeeping before anything
//! runs, the notice that the host is back, the background loops, the TLS pair
//! and the systemd watchdog.

use crate::*;

/// H5: the self-update gate runs `staged --selfcheck` before installing.
/// Prove we can execute at all and report our version, then exit. `--version`
/// does the same for a person. Any other argument stops the daemon before it
/// opens a port: on 2026-09-27 `homelab-host --version` run by hand on pve
/// started a second daemon that held 8443 while systemd restarted the real
/// one (corr-rogue-daemon).
pub(crate) fn check_arguments() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {}
        // `--selfcheck` stays the bare version: the self-update reads it.
        Some("--selfcheck") if args.len() == 1 => {
            println!("{}", VERSION);
            std::process::exit(0);
        }
        // fix-141: a person asking also learns which tree it was built from.
        Some("--version") if args.len() == 1 => {
            println!("{} ({})", VERSION, BUILD);
            std::process::exit(0);
        }
        Some(_) => {
            eprintln!(
                "homelab-host: unknown argument(s) {:?}; it takes none (only --version or --selfcheck) and is started by systemd",
                args
            );
            std::process::exit(2);
        }
    }
}

/// Seconds since the epoch; 0 when the clock is before it.
fn now_unix() -> u64 {
    unix_now()
}

/// What the previous run left behind, dealt with before anything runs.
pub(crate) fn housekeeping(config: &Config) {
    // fix-125: records written before the private modes existed.
    tighten_private_paths(&config.state_dir);
    // fix-131: bound the daemon's own records before anything runs, so
    // nothing writes the journal while it is cut.
    compact_journal_file(&config.state_dir, config.journal_max_bytes);
    prune_incidents(
        &config.state_dir,
        now_unix(),
        config.incident_bundle_max_age_days,
        config.incident_bundle_max_count,
    );
    // rule-20: the previous run's orphaned push-staging files, if any.
    cleanup_push_staging(&config.state_dir, now_unix());
}

/// AR13: surface any operation the previous run left mid-flight.
pub(crate) fn interrupted_operations(config: &Config) -> Vec<String> {
    let mut interrupted: Vec<String> = Vec::new();
    if let Ok(journal) = std::fs::read_to_string(format!("{}/journal.jsonl", config.state_dir)) {
        for (op, step) in homelab_core::incidents::interrupted_ops(&journal) {
            tracing::warn!(
                "interrupted operation '{}' at step '{}' — re-running it is safe (idempotent)",
                op,
                step
            );
            interrupted.push(format!("{} @ {}", op, step));
        }
    }
    interrupted
}

/// F3: boot notification — after a power cut or crash-restart, the
/// dashboard's centre hears that the daemon is back and whether anything was
/// left mid-flight. Delayed so the network is up.
pub(crate) fn spawn_boot_notice(state: &AppState, interrupted: Vec<String>) {
    let boot_state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let err = if interrupted.is_empty() {
            None
        } else {
            Some(format!("interrupted: {}", interrupted.join("; ")))
        };
        // Decision notify-routing (2026-09-30): a notice in the
        // dashboard's centre; pushed only when work was interrupted
        // (push-edge, 2026-09-30).
        let ok = interrupted.is_empty();
        let ex = homelab_core::notify::explain_event("host-online", "boot", ok, err.as_deref());
        publish_notice(
            &boot_state,
            &RealExecutor,
            NoticeFacts {
                op: "host-online".into(),
                label: "boot".into(),
                ok,
                deferred: false,
                since: boot_state.started_at,
                ex,
                urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Boot {
                    interrupted: !ok,
                }),
                incident: None,
                req: None,
                by: None,
                findings: Vec::new(),
            },
        )
        .await;
    });
}

/// The loops that run beside the server: container status readings, the
/// snapshot cache sweep, the dashboard watch and the home-address sync.
pub(crate) fn spawn_background(state: &AppState) {
    // feat-platform-2: read every container's real status on a timer, so
    // GetState answers from the newest reading instead of fixed values.
    {
        let st = state.clone();
        tokio::spawn(async move { status_loop(st).await });
    }
    // fix-180: keep every repository's snapshot cache fresh in the
    // background, bounded and coalesced, so `BackupCalendar`/`GetBackups`
    // never shell out to restic on the request path.
    {
        let st = state.clone();
        tokio::spawn(async move { snapshot_cache_sweep_loop(st).await });
    }
    // replace-kuma (Kenny, 2026-09-30): the host watches the dashboard, as
    // the dashboard watches the host.
    if let Some(url) = state.config.watch_url.clone() {
        let st = state.clone();
        tokio::spawn(async move { watch_dashboard(st, url).await });
    }
    // fix-156: read the house's address once at start, so a restart (a
    // release, a power cut) does not leave the dashboard's second lock and
    // CrowdSec's whitelist without it until the night.
    {
        let st = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            run_mutating_op(&st, &RealExecutor, 0, "home-address-whitelist", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await },
                )
            })
            .await;
        });
    }
}

/// Arm the scheduler task; `supervise` ends the process if it ends (fix-52).
pub(crate) fn spawn_scheduler(state: &AppState) -> tokio::task::JoinHandle<()> {
    let sched_state = state.clone();
    let handle = tokio::spawn(async move { scheduler_loop(sched_state).await });
    match state.config.initial_settings.backup_hour {
        Some(hour) => info!(
            "scheduler armed: daily backup + auto-updates at {:02}:00",
            hour
        ),
        None => info!("scheduler idle (backup_hour not set)"),
    }
    handle
}

/// A4: TLS with a self-signed cert; the client pins this fingerprint.
/// fix-128: a clear line instead of a panic when even a new pair cannot be
/// made or loaded (a full or read-only disk); systemd restarts and the journal
/// says why.
pub(crate) async fn tls_config(config: &Config) -> axum_server::tls_rustls::RustlsConfig {
    let (certs, fingerprint) = match tls::ensure_cert(&config.state_dir, "homelab-host") {
        Ok(pair) => pair,
        Err(e) => {
            error!(
                "FATAL: cannot make or read the TLS certificate in {} :: {} — check that the \
                 directory is writable and the disk is not full",
                config.state_dir, e
            );
            std::process::exit(1);
        }
    };
    info!(
        "homelab-host v{} ({}) listening on {} (TLS)",
        VERSION, BUILD, config.listen
    );
    info!("TLS fingerprint SHA256:{}", fingerprint);
    let tls_config =
        match axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.cert_pem, &certs.key_pem)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "FATAL: {} and {} do not load as a TLS pair :: {} — move both aside and \
                 restart; the daemon makes a new pair (clients must then re-pin)",
                    certs.cert_pem, certs.key_pem, e
                );
                std::process::exit(1);
            }
        };
    tls_config
}

/// H5 / fix-121: a pending self-update is accepted by the first authenticated
/// request this daemon answers (`accept_pending_update`), no longer by five
/// seconds alive. Until then the marker stays armed, so a binary that binds
/// and then dies is still rolled back by OnFailure.
pub(crate) fn note_pending_update(config: &Config) {
    if std::path::Path::new(&format!("{}/selfupdate.pending", config.state_dir)).exists() {
        info!(
            "self-update pending — v{} is accepted by the first authenticated request it \
             answers; until then a crash-loop rolls it back",
            VERSION
        );
    }
}

/// B7: feed systemd's watchdog. fix-52 residual: the ping is withheld once the
/// scheduler's own heartbeat goes stale, so systemd's `WatchdogSec` restarts a
/// daemon whose scheduler stopped making progress, not just one whose process
/// stopped existing.
pub(crate) fn spawn_watchdog(heartbeat: std::sync::Arc<std::sync::atomic::AtomicU64>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let last = heartbeat.load(std::sync::atomic::Ordering::Relaxed);
            if scheduler_is_alive(now, last, SCHEDULER_WATCHDOG_STALE_S) {
                sd_notify("WATCHDOG=1");
            } else {
                tracing::error!(
                    "scheduler heartbeat is {} s old — withholding WATCHDOG=1 so systemd \
                         restarts a daemon whose nightly round has stalled",
                    now.saturating_sub(last)
                );
            }
        }
    });
}
