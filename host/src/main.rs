//! Homelab HOST daemon — thin shell around homelab-core (AR1).
//!
//! Provides: the real Executor (processes + files), config (TOML + env,
//! AR11), tracing (AR15), the journal file (B5), the WS server with required
//! bearer token, and the broadcast sink feeding connected clients (F2).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use tokio::process::Command;
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info};

use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor};
use homelab_core::ops::{OpCtx, deploy::deploy};
use homelab_core::runner::Journal;
use homelab_core::sink::{PipelineEvent, Sink};

use asker::*;
use auth::*;
use backup_batch::*;
use cache_rounds::*;
use catchup::*;
use config::*;
use dashboard_watch::*;
use drills::*;
use edge::*;
use exec_guard::*;
use executor::*;
use fleet_state::*;
use gather_capacity::*;
use gather_snapshots::*;
use gather_today::*;
use http::*;
use incidents::*;
use journal::*;
use logging::*;
use native_state::*;
use nightly::*;
use nightly_tools::*;
use notices::*;
use notify_send::*;
use op_lock::*;
use probes::*;
use rpc::*;
use scheduler::*;
use sink::*;
use state::*;
use supervise::*;
use ws::*;

use homelab_core::ops::backup::NightBackup;

use homelab_proto::{Command as Rpc, RpcRequest, RpcResponse, ServerMsg};

mod asker;
mod auth;
mod backup_batch;
mod cache_rounds;
mod catchup;
mod config;
mod dashboard_watch;
mod drills;
mod edge;
mod exec_guard;
mod executor;
mod fleet_state;
mod gather_capacity;
mod gather_snapshots;
mod gather_today;
mod http;
mod incidents;
mod journal;
mod logging;
mod native_state;
mod nightly;
mod nightly_tools;
mod notices;
mod notify_send;
mod op_lock;
mod probes;
mod rpc;
mod scheduler;
mod secrets;
mod sink;
mod state;
mod supervise;
mod tls;
mod ui_relay;
mod ws;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// fix-141: the tree this binary was built from (see build.rs).
const BUILD: &str = env!("HOMELAB_BUILD");

#[cfg(test)]
mod tests;

// ── Real executor (AR2) ─────────────────────────────────────────────────────

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RingWriter {
    type Writer = RingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::main]
async fn main() {
    // H5: the self-update gate runs `staged --selfcheck` before installing.
    // Prove we can execute at all and report our version, then exit.
    // `--version` does the same for a person. Any other argument stops the
    // daemon before it opens a port: on 2026-09-27 `homelab-host --version`
    // run by hand on pve started a second daemon that held 8443 while systemd
    // restarted the real one (corr-rogue-daemon).
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

    // fix-122 (see REGISTER.md): config loads first, so the subscriber can
    // start with `state_dir` (the ring's home) and `log_level` (its seed
    // filter) already known. `load_config` never logs — a parse failure or
    // a refused value goes to stderr directly and exits — so nothing is
    // lost by moving it ahead of the subscriber.
    let config = load_config();
    let log_filter = init_production_logging(
        &config.state_dir,
        &config.log_level,
        config.log_ring_max_bytes,
    );

    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState::new(config.clone(), log_tx).with_log_filter(log_filter);

    // fix-125: records written before the private modes existed.
    tighten_private_paths(&config.state_dir);
    // fix-131: bound the daemon's own records before anything runs, so
    // nothing writes the journal while it is cut.
    compact_journal_file(&config.state_dir, config.journal_max_bytes);
    prune_incidents(
        &config.state_dir,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        config.incident_bundle_max_age_days,
        config.incident_bundle_max_count,
    );
    // rule-20: the previous run's orphaned push-staging files, if any.
    cleanup_push_staging(
        &config.state_dir,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );

    // AR13: surface any operation the previous run left mid-flight.
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

    // F3: boot notification — after a power cut or crash-restart, the
    // dashboard's centre hears that the daemon is back and whether anything
    // was left mid-flight. Delayed so the network is up.
    {
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

    // E4: nightly scheduler — backups for every managed stack + auto-policy
    // updates, driven from state.json manifests (no client needed). Reads the
    // live settings each tick, so G8 edits apply without a restart.
    // fix-52: the handle is kept; `supervise` ends the process if it ends.
    let scheduler = {
        let sched_state = state.clone();
        let handle = tokio::spawn(async move { scheduler_loop(sched_state).await });
        match config.initial_settings.backup_hour {
            Some(hour) => info!(
                "scheduler armed: daily backup + auto-updates at {:02}:00",
                hour
            ),
            None => info!("scheduler idle (backup_hour not set)"),
        }
        handle
    };
    let op_lock = state.op_lock.clone();
    // fix-52 residual: taken before `state` moves into `app_router`.
    let scheduler_heartbeat = state.scheduler_heartbeat.clone();

    let app = app_router(state);

    // A4: TLS with a self-signed cert; the client pins this fingerprint.
    // fix-128: a clear line instead of a panic when even a new pair cannot be
    // made or loaded (a full or read-only disk); systemd restarts and the
    // journal says why.
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

    // H5 / fix-121: a pending self-update is accepted by the first
    // authenticated request this daemon answers (`accept_pending_update`),
    // no longer by five seconds alive. Until then the marker stays armed, so
    // a binary that binds and then dies is still rolled back by OnFailure.
    if std::path::Path::new(&format!("{}/selfupdate.pending", config.state_dir)).exists() {
        info!(
            "self-update pending — v{} is accepted by the first authenticated request it \
             answers; until then a crash-loop rolls it back",
            VERSION
        );
    }

    // fix-52: bind first, so READY=1 means the port is open. It was sent
    // before the socket existed, and a bind failure then looked like a
    // daemon that had started.
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .and_then(|l| l.into_std())
        .expect("bind listen address");
    let server = axum_server::from_tcp_rustls(listener, tls_config).expect("serve");

    // B7: tell systemd we're ready, then feed its watchdog. If this loop
    // ever stops (deadlock/hang), systemd kills and restarts the daemon.
    //
    // fix-52 residual: a ping every 10 s used to prove only that THIS loop
    // was still scheduled, which says nothing about the scheduler — a
    // nightly round wedged inside one `await` (not a panic; `supervise()`
    // already turns a panic or a dead task into exit 1) kept being fed
    // forever. Now the ping is withheld once the scheduler's own heartbeat
    // goes stale, so systemd's `WatchdogSec` eventually restarts a daemon
    // whose scheduler stopped making progress, not just one whose process
    // stopped existing.
    sd_notify("READY=1");
    {
        let heartbeat = scheduler_heartbeat;
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

    let code = supervise(
        // fix-120: with the peer address, so a refused connection says where
        // it came from.
        server.serve(app.into_make_service_with_connect_info::<SocketAddr>()),
        scheduler,
        terminate_signal(),
        op_lock,
        Duration::from_secs(60),
    )
    .await;
    std::process::exit(code);
}
