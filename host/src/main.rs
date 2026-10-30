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
use notices::*;
use notify_send::*;
use op_lock::*;
use probes::*;
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
mod notices;
mod notify_send;
mod op_lock;
mod probes;
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

async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor;
    match req.command {
        // Answered by the session loop itself; reaching here means a caller
        // bypassed it (a test harness), which changes nothing.
        // arch-history: what the host did since a moment.
        Rpc::History {
            since,
            until,
            limit,
        } => {
            let text = std::fs::read_to_string(format!("{}/history.jsonl", state.config.state_dir))
                .unwrap_or_default();
            let entries = homelab_core::history::select_between(
                homelab_core::history::parse(&text),
                since,
                until,
                limit,
            );
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "entries": entries }).to_string(),
                deferred: None,
            }
        }
        // Decision notify-routing: the notices after the dashboard's cursor.
        Rpc::Notices { after, limit } => {
            let text =
                std::fs::read_to_string(notices_path(&state.config.state_dir)).unwrap_or_default();
            let all = homelab_core::notify::parse_notices(&text);
            let last_seq = all.iter().map(|n| n.seq).max().unwrap_or(0);
            let notices = homelab_core::notify::notices_after(all, after, limit.min(1000));
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "notices": notices, "last_seq": last_seq })
                    .to_string(),
                deferred: None,
            }
        }
        // feat-platform-3: what runs now, and the newest lines.
        Rpc::CurrentOp => {
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
                id: req.id,
                ok: true,
                message: serde_json::to_string(&view).unwrap_or_default(),
                deferred: None,
            }
        }
        Rpc::SessionOptions { .. } => RpcResponse {
            id: req.id,
            ok: true,
            message: "session options are set per session".into(),
            deferred: None,
        },
        // feat-platform-10: answered by the session loop (`serve_ws`).
        Rpc::Ui { .. } | Rpc::UiAttach | Rpc::UiReply { .. } | Rpc::UiHold { .. } => RpcResponse {
            id: req.id,
            ok: false,
            message: "UI steps are relayed by the session, not run".into(),
            deferred: None,
        },
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
            deferred: None,
        },
        // fix-68 (see REGISTER.md): hands back the same fleet snapshot
        // `homelab ui`/the dashboard use, which the client turns
        // into a short human table, or prints as-is for `--json`.
        Rpc::Status => {
            let fleet = build_fleet_state(state, &exec).await;
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&fleet).unwrap_or_else(|_| "{}".into()),
                deferred: None,
            }
        }
        // T85: a native binary arrives on its own, before the deploy that
        // installs it. Kept under the state directory, root-only, until the
        // deploy consumes it; a name that is not a plain stack or unit name
        // is refused before it can become a path.
        Rpc::StageNativeBinary {
            stack,
            unit,
            binary_b64,
        } => {
            let plain = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            };
            if !plain(&stack) || !plain(&unit) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "refusing to stage '{}' for '{}': stack and unit names are lowercase \
                         [a-z0-9-] and nothing else",
                        unit, stack
                    ),
                    deferred: None,
                };
            }
            let dir = staged_binaries_dir(&state.config.state_dir, &stack);
            let path = format!("{}/{}.b64", dir, unit);
            // fix-51: through the executor's atomic write (own temp name,
            // 0600 from creation, off the async worker) rather than a plain
            // `std::fs::write` of tens of MB that a deploy reading the file at
            // the same moment could see half-written.
            let written = exec.write_file(&path, &binary_b64, 0o600).await;
            match written {
                Ok(()) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "staged {} for {} ({} KiB of base64) — the next deploy of this stack \
                         installs it",
                        unit,
                        stack,
                        binary_b64.len() / 1024
                    ),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("could not stage {} for {}: {}", unit, stack, e),
                    deferred: None,
                },
            }
        }
        Rpc::DeployStack(mut spec) => {
            // T85: the binaries this deploy installs were staged one per
            // message; fill them in here, and clear the staging area after
            // the deploy whatever its outcome — a retry stages again.
            let dir = staged_binaries_dir(&state.config.state_dir, &spec.manifest.stack_name);
            let merged = homelab_core::ops::native::merge_staged_binaries(
                &spec.manifest.natives,
                &mut spec.native_binaries,
                |unit| std::fs::read_to_string(format!("{}/{}.b64", dir, unit)).ok(),
            );
            if !merged.is_empty() {
                info!(
                    "deploy {}: {} staged binar{} taken up ({})",
                    spec.manifest.stack_name,
                    merged.len(),
                    if merged.len() == 1 { "y" } else { "ies" },
                    merged.join(", ")
                );
            }
            let vmid = spec.manifest.vmid;
            let stack_name = spec.manifest.stack_name.clone();
            let resp = run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "deploy",
                Some(&stack_name),
                |ctx| Box::pin(async move { deploy(ctx, &spec).await }),
            )
            .await;
            let _ = std::fs::remove_dir_all(&dir);
            // fix-94: every gateway deploy re-reads the house's address and
            // keeps CrowdSec's whitelist equal to it. Its own operation, so
            // its lines reach the client before this deploy's answer does,
            // and a failure there is reported as itself rather than as a
            // failed deploy.
            if home_address_after_deploy(vmid, resp.ok, state.config.safety.gateway_vmid) {
                run_mutating_op(state, &exec, req.id, "home-address-whitelist", |ctx| {
                    Box::pin(
                        async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await },
                    )
                })
                .await;
            }
            resp
        }
        Rpc::DestroyStack {
            manifest,
            confirm,
            skip_backup,
        } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy(ctx, &manifest, &confirm, skip_backup).await
                })
            })
            .await
        }
        Rpc::BackupStack(manifest) => {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: state
                    .settings
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retention
                    .clone(),
                // fix-223: an operator asked for this on demand.
                trigger: homelab_core::ops::backup::BackupTrigger::Manual,
                ..state.config.backup.clone()
            };
            let stack = manifest.stack_name.clone();
            // fix-180: the owners the cache refreshes on success — read
            // before `manifest` moves into the closure below.
            let owners: Vec<String> = homelab_core::ops::backup::owner_groups(&manifest)
                .into_iter()
                .map(|(owner, _paths)| owner)
                .collect();
            let resp =
                run_mutating_op_for_stack(state, &exec, req.id, "backup", Some(&stack), |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await
                    })
                })
                .await;
            if resp.ok {
                record_backup_time(state, &stack).await;
                kick_snapshot_refresh_many(state, state.config.backup.clone(), owners);
            }
            resp
        }
        Rpc::RestoreStack {
            manifest,
            snapshot,
            confirm,
            skip_safety_copy,
            app,
            source,
        } => {
            // fix-64: no typed name, no restore — whoever sent the request.
            if let Err(e) = homelab_core::ops::backup::restore_confirmed(
                &manifest.stack_name,
                confirm.as_deref(),
            ) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e.to_string(),
                    deferred: None,
                };
            }
            // The configured target and timeout, not the compiled defaults
            // (F38, see REGISTER.md); restore-two-sources-1: from the set the
            // operator picked.
            let cfg =
                match backup_cfg_for_source(&exec, state.config.backup.clone(), source.as_deref())
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: e,
                            deferred: None,
                        };
                    }
                };
            let stack_name = manifest.stack_name.clone();
            // fix-180: a restore may take a safety-copy backup first (unless
            // `skip_safety_copy`) and always re-tags a `restic forget` pass
            // over the repositories it touches, so the same owners are
            // refreshed on success as on an ordinary backup.
            let owners: Vec<String> = homelab_core::ops::backup::owner_groups(&manifest)
                .into_iter()
                .map(|(owner, _paths)| owner)
                .collect();
            let resp = run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "restore",
                Some(&stack_name),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::backup::restore_app(
                            ctx,
                            &manifest,
                            &cfg,
                            &snapshot,
                            !skip_safety_copy,
                            app.as_deref(),
                        )
                        .await
                    })
                },
            )
            .await;
            if resp.ok {
                kick_snapshot_refresh_many(state, state.config.backup.clone(), owners);
            }
            resp
        }
        Rpc::UpdateStack { manifest, app } => {
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "update", Some(&stack_name), |ctx| {
                Box::pin(async move {
                    homelab_core::ops::update::update(ctx, &manifest, app.as_deref(), false).await
                })
            })
            .await
        }
        Rpc::PatchFleet => {
            // Targets come from state.json — only stacks we deployed.
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await.unwrap_or_default();
            let targets: Vec<(String, u16)> = snapshot
                .stacks
                .iter()
                .map(|(name, st)| (name.clone(), st.vmid))
                .collect();
            run_mutating_op(state, &exec, req.id, "patch", |ctx| {
                Box::pin(async move { homelab_core::ops::patch::patch_fleet(ctx, &targets).await })
            })
            .await
        }
        Rpc::ApplyResources(manifest) => {
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "resize", Some(&stack_name), |ctx| {
                Box::pin(async move { homelab_core::ops::resize::hot_apply(ctx, &manifest).await })
            })
            .await
        }
        Rpc::BackupHostMeta => {
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers,
                ..state.config.backup.clone()
            };
            let resp = run_mutating_op(state, &exec, req.id, "host-meta-backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await },
                )
            })
            .await;
            if resp.ok {
                let store =
                    homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                record_state(&store, "last_host_meta", |s| s.last_host_meta = now).await;
            }
            resp
        }
        Rpc::AdoptService(m) => {
            let stack_name = m.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "adopt", Some(&stack_name), |ctx| {
                Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
            })
            .await
        }
        Rpc::InstallNative {
            manifest,
            binary_b64,
            unit_file,
        } => {
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "install-native",
                Some(&stack_name),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::install_native(
                            ctx,
                            &manifest,
                            &binary_b64,
                            &unit_file,
                        )
                        .await
                    })
                },
            )
            .await
        }
        // TUI parity round: the dashboard's install-native; the host fetches
        // and verifies the release itself (CT 120 has no gh).
        Rpc::InstallNativeRelease {
            manifest,
            unit_file,
            tag,
        } => {
            if let Err(problems) = homelab_core::native::validate_native(&manifest) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("service.yml invalid: {}", problems.join("; ")),
                    deferred: None,
                };
            }
            if manifest.release_repo.is_none() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "{} declares no release_repo — there is no release to install; add \
                         release_repo to its service.yml",
                        manifest.unit
                    ),
                    deferred: None,
                };
            }
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "install-native",
                Some(&stack_name),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::install_release(ctx, &manifest, &tag, &unit_file)
                            .await
                    })
                },
            )
            .await
        }
        Rpc::BackupNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, _)) => {
                    let tiers = state
                        .settings
                        .read()
                        .unwrap_or_else(PoisonError::into_inner)
                        .retention
                        .clone();
                    let cfg = homelab_core::ops::backup::BackupCfg {
                        tiers,
                        // fix-223: an operator asked for this on demand.
                        trigger: homelab_core::ops::backup::BackupTrigger::Manual,
                        ..state.config.backup.clone()
                    };
                    // T5: the stack may hold several services; back up each,
                    // and report the first failure rather than the last.
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                        deferred: None,
                    };
                    for m in services {
                        let cfg = cfg.clone();
                        // fix-180: the unit name IS the repository's owner
                        // for a native service (no `owner_groups` — that is
                        // compose-only, D25).
                        let owner = m.unit.clone();
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "backup-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::backup_native(ctx, &m, &cfg).await
                                })
                            },
                        )
                        .await;
                        let failed = !r.ok;
                        if r.ok {
                            kick_snapshot_refresh(state, state.config.backup.clone(), owner);
                        }
                        if resp.ok || failed {
                            resp = r;
                        }
                        if failed {
                            break;
                        }
                    }
                    if resp.ok {
                        record_backup_time(state, &stack).await;
                    }
                    resp
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                    deferred: None,
                },
            }
        }
        Rpc::UpdateNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, applied_at)) => {
                    let stored_at = Some(applied_at);
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                        deferred: None,
                    };
                    for m in services {
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "update-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::update_native(ctx, &m, stored_at)
                                        .await
                                })
                            },
                        )
                        .await;
                        let failed = !r.ok;
                        if resp.ok || failed {
                            resp = r;
                        }
                        if failed {
                            break;
                        }
                    }
                    resp
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                    deferred: None,
                },
            }
        }
        Rpc::RollbackNative { stack, unit } => {
            match native_from_state(&state.config.state_dir, &stack)
                .await
                .and_then(|(services, _)| {
                    homelab_core::ops::native::select_unit(&services, unit.as_deref())
                }) {
                Ok(m) => {
                    run_mutating_op_for_stack(
                        state,
                        &exec,
                        req.id,
                        "rollback-native",
                        Some(&stack),
                        |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::rollback_native(ctx, &m).await
                            })
                        },
                    )
                    .await
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                    deferred: None,
                },
            }
        }
        Rpc::ReleaseUpdateNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, _)) => {
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                        deferred: None,
                    };
                    for m in services {
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "release-update-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::release_update(ctx, &m).await
                                })
                            },
                        )
                        .await;
                        let failed = !r.ok;
                        if resp.ok || failed {
                            resp = r;
                        }
                        if failed {
                            break;
                        }
                    }
                    resp
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                    deferred: None,
                },
            }
        }
        Rpc::PruneOrphans {
            manifest,
            spec,
            confirm,
        } => {
            if confirm != manifest.stack_name {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "confirmation '{}' does not match stack '{}' — nothing removed",
                        confirm, manifest.stack_name
                    ),
                    deferred: None,
                };
            }
            // A1/A2: the same gate every mutating operation passes. Removing
            // files reaches into a container, so the no-touch list and the
            // hostname check apply exactly as they do to a deploy.
            if let Err(e) =
                homelab_core::safety::check_deploy_target(&exec, &state.config.safety, &manifest)
                    .await
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                    deferred: None,
                };
            }
            // The same exclusion the deploy applies: the gateway holds other
            // stacks' routes.
            let keep =
                homelab_core::ops::deploy::generated_dirs(&state.config.safety, manifest.vmid);
            let orphans =
                homelab_core::ops::deploy::orphan_files_keeping(&exec, &manifest, &spec, &keep)
                    .await;
            if orphans.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "nothing to remove — everything under /opt/{} is in the repository",
                        manifest.stack_name
                    ),
                    deferred: None,
                };
            }
            let mut removed = 0usize;
            for o in &orphans {
                let path = format!("/opt/{}/{}", manifest.stack_name, o);
                // -f, never -r: this removes FILES the repository dropped.
                // A directory would take whatever else is under it, which is
                // exactly the surprise this whole feature exists to avoid.
                if exec
                    .run(&homelab_core::executor::Cmd::new(
                        "pct",
                        &["exec", &manifest.vmid.to_string(), "--", "rm", "-f", &path],
                        60,
                    ))
                    .await
                    .is_ok()
                {
                    removed += 1;
                    tracing::info!("[prune] removed {}", path);
                }
            }
            RpcResponse {
                id: req.id,
                ok: removed == orphans.len(),
                message: format!("removed {} of {} orphan file(s)", removed, orphans.len()),
                deferred: None,
            }
        }
        // step-22: forget runs the same unregister steps destroy runs
        // (route, scrape target, dashboard, manual checks, front page, host
        // monitors) and records what the stack left behind (ask-9) — under
        // the op-lock like every other operation that writes state.
        Rpc::ForgetStack { stack } => {
            run_mutating_op(state, &exec, req.id, "forget", |ctx| {
                Box::pin(async move { homelab_core::ops::destroy::forget(ctx, &stack).await })
            })
            .await
        }
        // ask-8: `homelab apply` destroys a stack whose directory is gone,
        // from the manifest recorded in state. Every gate of a destroy holds.
        Rpc::DestroyRecorded {
            stack,
            confirm,
            skip_backup,
        } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy_recorded(ctx, &stack, &confirm, skip_backup)
                        .await
                })
            })
            .await
        }
        // ask-9: what a retired entry kept, listed or wiped.
        Rpc::WipeRetired { name, confirm } => match confirm {
            None => {
                let snapshot = match homelab_core::state::StateStore::new(
                    &RealExecutor,
                    &state.config.state_dir,
                )
                .load()
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: format!("state unreadable: {}", e),
                            deferred: None,
                        };
                    }
                };
                match homelab_core::ops::retired::wipe_plan(
                    &snapshot,
                    &name,
                    &state.config.state_dir,
                ) {
                    Ok(plan) => RpcResponse {
                        id: req.id,
                        ok: true,
                        message: plan.render(&name),
                        deferred: None,
                    },
                    Err(why) => RpcResponse {
                        id: req.id,
                        ok: false,
                        message: why,
                        deferred: None,
                    },
                }
            }
            Some(confirm) => {
                run_mutating_op(state, &exec, req.id, "wipe", |ctx| {
                    Box::pin(
                        async move { homelab_core::ops::retired::wipe(ctx, &name, &confirm).await },
                    )
                })
                .await
            }
        },
        Rpc::ApplyGuards { vmid } => {
            // A1 still governs: the guards write files and restart docker, so
            // an untouchable guest is untouchable here too.
            if state.config.safety.no_touch.contains(&vmid) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "vmid {} is on the no-touch list :: this list is the one thing that is never worked around",
                        vmid
                    ),
                    deferred: None,
                };
            }
            run_mutating_op(state, &exec, req.id, "apply-guards", |ctx| {
                Box::pin(async move {
                    let mut runner =
                        homelab_core::runner::Runner::new("apply-guards", ctx.sink, ctx.journal);
                    // fix-171 round 3: one fixed step, always.
                    runner.plan(homelab_core::ops::guards::APPLY_GUARDS_STEPS);
                    match runner
                        .step("guards", || async {
                            // gap-33: only a managed stack, after the A2
                            // hostname guard, docker guards only where it
                            // runs docker.
                            let store = homelab_core::state::StateStore::new(ctx.exec, &ctx.state_dir);
                            let snapshot = store.load().await.unwrap_or_default();
                            let managed: Vec<homelab_core::ops::guards::ManagedTarget> = snapshot
                                .stacks
                                .iter()
                                .map(|(name, st)| homelab_core::ops::guards::ManagedTarget {
                                    name: name.clone(),
                                    vmid: st.vmid,
                                    // A stack with no stored manifest was adopted
                                    // (`homelab adopt` stores none): a native
                                    // service, so no docker guards (CT 118).
                                    docker: st.manifest.as_ref().map(|m| !m.native_only).unwrap_or(false),
                                })
                                .collect();
                            homelab_core::ops::guards::apply_for_managed(
                                ctx.exec,
                                ctx.sink,
                                &ctx.safety,
                                &managed,
                                vmid,
                                ctx.registry_cache.as_ref(),
                            )
                            .await?;
                            Ok(homelab_core::runner::StepOutcome::Changed)
                        })
                        .await
                    {
                        Ok(_) => {
                            runner.log(
                                homelab_core::sink::Level::Info,
                                format!(
                                    "[guards] {} — runaway guards applied (the docker ones only where the stack runs docker)",
                                    vmid
                                ),
                            );
                            runner.finish_ok()
                        }
                        Err(e) => runner.finish_err("guards", &e),
                    }
                })
            })
            .await
        }
        Rpc::FleetCheck {
            stack_files,
            digests,
            json,
            host_config,
        } => {
            let mut live = gather_live_facts(&exec, state, &stack_files, false).await;
            // fix-142: what the client's files say, for the repository comparison.
            live.digests = digests;
            // fix-110: as above, for config/host.toml against this host's own.
            live.declared_host_config = host_config;
            live.live_host_config = live_host_config_table(
                &std::fs::read_to_string(&state.config.config_path).unwrap_or_default(),
            );
            let snapshot =
                match homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir)
                    .load()
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: format!("state unreadable: {}", e),
                            deferred: None,
                        };
                    }
                };
            let findings = homelab_core::ops::fleetcheck::evaluate(
                &snapshot,
                &live,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                state.config.backup_max_age_s,
                homelab_core::ops::fleetcheck::GrowthLimits::default(),
                state.config.tile_watch_source.as_deref(),
                &state.config.trusted_sources,
                state.config.patch_threshold_s,
                state.config.host_meta_max_age_s,
                state.config.capacity_thresholds,
            );
            RpcResponse {
                id: req.id,
                ok: homelab_core::ops::fleetcheck::check_passes(&findings),
                message: if json {
                    serde_json::json!({
                        "passes": homelab_core::ops::fleetcheck::check_passes(&findings),
                        "findings": findings,
                    })
                    .to_string()
                } else {
                    render_findings(&findings)
                },
                deferred: None,
            }
        }
        // fix-68: always ok with a JSON body. A part that could not be read
        // travels inside it as `unread`, so the TUI can tell this reply from
        // any other by its shape and never mistakes a failure of it for the
        // end of an operation it has open.
        Rpc::Today {
            stack_files,
            digests,
            host_config,
        } => RpcResponse {
            id: req.id,
            ok: true,
            message: serde_json::to_string(
                &gather_today(&exec, state, &stack_files, digests, host_config).await,
            )
            .unwrap_or_default(),
            deferred: None,
        },
        // T69: the operator answered a suspended step. Delivering it is all
        // that happens here — the step itself is parked on a channel inside
        // the operation, not on this task.
        Rpc::Answer { id, allow, boot } => {
            // arch-host-link: an answer to a question from an earlier start of
            // this host must not answer today's question with the same id.
            if boot.as_ref().is_some_and(|b| *b != state.boot_id) {
                return RpcResponse {
                    id: req.id,
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
                id: req.id,
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
        // fix-240: answer a running operation's question from ANY session —
        // a headless run cannot answer its own, a second client or Live view
        // can. The same delivery as `Answer`, found by operation rather than
        // by an id only the asking session saw.
        Rpc::AnswerOpen { op, allow } => {
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
                id: req.id,
                ok,
                message,
                deferred: None,
            }
        }
        Rpc::ZfsReplicate => {
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let jobs = state.config.zfs_jobs.clone();
            let resp = run_mutating_op(state, &exec, req.id, "zfs-replicate", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::zfs::replicate(
                        ctx,
                        &jobs,
                        &tiers,
                        homelab_core::ops::backup::BackupTrigger::Manual,
                    )
                    .await
                })
            })
            .await;
            if resp.ok {
                let store =
                    homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                record_state(&store, "last_zfs", |s| s.last_zfs = now).await;
            }
            resp
        }
        Rpc::BackupDevices => {
            let devices = state.config.device_backups.clone();
            if devices.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "no device_backups configured in host.toml".into(),
                    deferred: None,
                };
            }
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let mut lines = Vec::new();
            let mut ok = true;
            for dev in devices {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let name = dev.name.clone();
                let report = run_mutating_op(state, &exec, req.id, "device-backup", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
                    })
                })
                .await;
                ok &= report.ok;
                if report.ok {
                    record_device_backup(state, &name).await;
                }
                lines.push(format!(
                    "{}: {}",
                    name,
                    if report.ok { "ok" } else { &report.message }
                ));
            }
            RpcResponse {
                id: req.id,
                ok,
                message: lines.join("\n"),
                deferred: None,
            }
        }
        Rpc::Tiles { bare } => {
            // The plain executor: a reading may read a key on its way.
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let mut st = store.load().await.unwrap_or_default();
            if bare {
                for s in st.stacks.values_mut() {
                    if let Some(m) = s.manifest.as_mut() {
                        for t in m.tiles.values_mut() {
                            t.reading = None;
                        }
                    }
                }
            }
            let tiles = homelab_core::ops::tiles::read_tiles(&RealExecutor, &st).await;
            // Decision "deploys are known outages" (Kenny, 2026-09-30): AR12
            // holds the operation lock strictly one at a time, so at most
            // one stack is ever deploying from the host's own view.
            let deploying_stack = state
                .busy
                .lock()
                .ok()
                .and_then(|b| b.as_ref().and_then(|h| h.stack.clone()));
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({
                    "tiles": tiles,
                    "deploying_stack": deploying_stack,
                    "watch_interval_s": state.config.watch_interval_s,
                    "watch_down_after_s": state.config.watch_down_after_s,
                })
                .to_string(),
                deferred: None,
            }
        }
        // feat-overview-10 (homelab-admin, 2026-10-01): the dashboard's
        // backup calendar, its own query reading restic directly (not
        // through state.json's cached `last_backup`, which only ever holds
        // the newest night — a calendar needs every one of them).
        //
        // fix-180: answered from `state.snapshot_cache`, never restic
        // directly — a whole-fleet read used to run every owner's
        // `restic snapshots` one after another and, live on CT 120
        // (2026-10-02), ran past the dashboard's 180 s budget and answered
        // 502 with nothing to say which stack it was on. A repository never
        // read before answers empty at once ("not read yet") while a
        // background read (bounded, coalesced — see `SnapshotCache`) fills
        // it in for the next call.
        Rpc::BackupCalendar { stacks, force } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let st = store.load().await.unwrap_or_default();
            let cfg = state.config.backup.clone();
            let wanted: Vec<&String> = if stacks.is_empty() {
                st.stacks.keys().collect()
            } else {
                stacks.iter().collect()
            };
            let mut by_stack = serde_json::Map::new();
            let mut measured_at = serde_json::Map::new();
            let mut skipped = Vec::new();
            // fix-202: a name that will NEVER get a restic read (no manifest
            // on record yet, not a known stack, or it backs up nothing by
            // design) used to fall straight through `continue` with no
            // `measured_at` entry at all — indistinguishable, client-side,
            // from an owner whose cache is simply still warming up. The
            // dashboard's per-stack read (fix-177) then retried it for the
            // full `UNREAD_GIVE_UP_MS` (3 min) before giving up with a
            // generic "press Refresh" message, which is exactly the "stuck
            // at 11 of 13 for minutes" symptom Kenny measured live on
            // 2026-10-02. Every such name now gets `measured_at` set at once
            // (so the client never waits on it) plus its own entry here:
            // `no_backup` for "nothing to keep, by design" (shown as such,
            // not as a failure), `reasons` for the rest (a real finding,
            // shown with its own text instead of a generic one).
            let mut no_backup = Vec::new();
            let mut reasons = serde_json::Map::new();
            let now = unix_now();
            for name in wanted {
                let Some(entry) = st.stacks.get(name) else {
                    reasons.insert(name.clone(), serde_json::json!("not a known stack"));
                    measured_at.insert(name.clone(), serde_json::json!(now));
                    skipped.push(format!("{name}: not a known stack"));
                    continue;
                };
                let Some(m) = entry.manifest.as_ref() else {
                    reasons.insert(name.clone(), serde_json::json!("no manifest on record"));
                    measured_at.insert(name.clone(), serde_json::json!(now));
                    skipped.push(format!("{name}: no manifest on record"));
                    continue;
                };
                if m.backs_up_nothing() {
                    no_backup.push(name.clone());
                    measured_at.insert(name.clone(), serde_json::json!(now));
                    continue;
                }
                let mut nights = Vec::new();
                let mut oldest: Option<u64> = None;
                let mut any_unread = false;
                for (owner, _paths) in homelab_core::ops::backup::owner_groups(m) {
                    match state.snapshot_cache.get(&owner) {
                        Some(cached) => {
                            nights.extend(cached.snapshots.iter().map(|s| s.time));
                            oldest =
                                Some(oldest.map_or(cached.measured_at, |o: u64| {
                                    o.min(cached.measured_at)
                                }));
                            if force {
                                kick_snapshot_refresh(state, cfg.clone(), owner);
                            }
                        }
                        None => {
                            any_unread = true;
                            kick_snapshot_refresh(state, cfg.clone(), owner);
                        }
                    }
                }
                if any_unread {
                    skipped.push(format!("{name}: not read yet"));
                }
                by_stack.insert(name.clone(), serde_json::json!(nights));
                measured_at.insert(
                    name.clone(),
                    oldest.map_or(serde_json::Value::Null, |t| serde_json::json!(t)),
                );
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({
                    "stacks": by_stack,
                    "measured_at": measured_at,
                    "skipped": skipped,
                    // fix-202: see above — always present (possibly empty),
                    // so the dashboard never has to tell "not answered yet"
                    // apart from "will never be answered".
                    "no_backup": no_backup,
                    "reasons": reasons,
                })
                .to_string(),
                deferred: None,
            }
        }
        // redesign-flows-6: one stack's containers and `manual` images,
        // read now (the dashboard's Update flow verifies a deploy with it).
        Rpc::StackRuntime { stack } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let st = store.load().await.unwrap_or_default();
            let read = match st.stacks.get(&stack) {
                None => Err(format!("the host records no stack {stack}")),
                Some(s) => homelab_core::ops::pins::read_runtime(&exec, s.vmid, unix_now()).await,
            };
            match read {
                Ok(rt) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::json!({ "stack": stack, "runtime": rt }).to_string(),
                    deferred: None,
                },
                Err(why) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: why,
                    deferred: None,
                },
            }
        }
        Rpc::ListManualChecks { json } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let st = store.load().await.unwrap_or_default();
            let rows = homelab_core::ops::manualchecks::listing(&st);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            RpcResponse {
                id: req.id,
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
        Rpc::AnswerManualCheck {
            check_id: id,
            ok,
            note,
            accept_days,
        } => {
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
                id: req.id,
                ok: found,
                message,
                deferred: None,
            }
        }
        Rpc::SetStackEnabled { stack, enabled } => {
            run_mutating_op(state, &exec, req.id, "set-enabled", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::enable::set_enabled(ctx, &stack, enabled).await
                })
            })
            .await
        }
        Rpc::SealEnv { stack } => {
            let report = run_mutating_op(state, &exec, req.id, "seal-env", |ctx| {
                Box::pin(async move { homelab_core::ops::sealenv::seal_env(ctx, &stack).await })
            })
            .await;
            // The fleet's "Env sealed" verdict is read again at once rather
            // than at the next slow tick, so the hub's row goes away.
            refresh_env_sealed(state, &exec).await;
            report
        }
        Rpc::ListTemplates => {
            // C5: discovery instead of hardcoded strings. Two sources: OS
            // tarballs (pveam) and clonable golden template containers.
            let tarballs = exec
                .run(&Cmd::new("pveam", &["list", "local"], 60))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let clones = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", "grep -l '^template: 1' /etc/pve/lxc/*.conf 2>/dev/null | while read f; do v=$(basename $f .conf); h=$(grep '^hostname:' $f | cut -d' ' -f2); echo \"clone:$v  $h\"; done"],
                    30,
                ))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let mut msg = String::from("clonable golden templates (fast):\n");
            msg.push_str(if clones.trim().is_empty() {
                "  (none — run 'homelab template-build')\n"
            } else {
                &clones
            });
            msg.push_str("\nOS templates (full bootstrap):\n");
            for line in tarballs.lines().skip(1) {
                if let Some(name) = line.split_whitespace().next() {
                    msg.push_str(&format!("  {}\n", name));
                }
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: msg,
                deferred: None,
            }
        }
        Rpc::BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
            base_template,
        } => {
            run_mutating_op(state, &exec, req.id, "template-build", |ctx| {
                Box::pin(async move {
                    let defaults = homelab_core::ops::template::TemplateCfg::default();
                    let cfg = homelab_core::ops::template::TemplateCfg {
                        temp_vmid,
                        version,
                        unprivileged,
                        base_template: base_template.unwrap_or(defaults.base_template),
                        ..defaults
                    };
                    homelab_core::ops::template::build_template(ctx, &cfg).await
                })
            })
            .await
        }
        Rpc::ExecIn { vmid, command } => {
            if let Err(e) = exec_allowed(&state.config, vmid) {
                return RpcResponse {
                    id: req.id,
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
            let key = (requested_by().unwrap_or_default(), req.id);
            let tag = format!("homelab-exec-{}-{}", req.id, ts);
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
            let ran =
                homelab_core::executor::pct_sh_tagged(&exec, vmid, &command, 120, Some(&tag)).await;
            let stopped = cancelled.load(std::sync::atomic::Ordering::SeqCst);
            match ran {
                Ok(out) => RpcResponse {
                    id: req.id,
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
                    id: req.id,
                    ok: false,
                    message: format!("{}{}", if stopped { "cancelled\n" } else { "" }, e),
                    deferred: None,
                },
            }
        }
        // Stop the exec a request id names, cleanly: TERM inside the
        // container, KILL after the grace `timeout -k` holds. The exec's own
        // reply then starts `cancelled`.
        Rpc::CancelExec { request } => {
            let by = requested_by().unwrap_or_default();
            let found = RUNNING_EXECS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&(by, request))
                .map(|r| (r.vmid, r.tag.clone(), r.cancelled.clone()));
            let Some((vmid, tag, cancelled)) = found else {
                return RpcResponse {
                    id: req.id,
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
                    id: req.id,
                    ok: true,
                    message: format!("stop signal sent to the command in CT {vmid}"),
                    deferred: None,
                },
                Ok(out) => {
                    cancelled.store(false, std::sync::atomic::Ordering::SeqCst);
                    RpcResponse {
                        id: req.id,
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
                        id: req.id,
                        ok: false,
                        message: format!("{e}"),
                        deferred: None,
                    }
                }
            }
        }
        Rpc::GetApplied { stack } => {
            // D6: the applied intent lives in the host repo; secrets never
            // do (A5), so this is safe to return.
            if stack.is_empty()
                || !stack
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "invalid stack name".into(),
                    deferred: None,
                };
            }
            let dir = format!("{}/repo/stacks/{}", state.config.state_dir, stack);
            let mut files: Vec<homelab_proto::FileBlob> = Vec::new();
            fn walk(
                base: &std::path::Path,
                dir: &std::path::Path,
                out: &mut Vec<homelab_proto::FileBlob>,
            ) {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            walk(base, &p, out);
                        } else if let Ok(content) = std::fs::read_to_string(&p) {
                            out.push(homelab_proto::FileBlob {
                                path: p.strip_prefix(base).unwrap().to_string_lossy().into_owned(),
                                content,
                                mode: None,
                            });
                        }
                    }
                }
            }
            walk(
                std::path::Path::new(&dir),
                std::path::Path::new(&dir),
                &mut files,
            );
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&files).unwrap_or_else(|_| "[]".into()),
                deferred: None,
            }
        }
        // fix-207: the live firewall status alone — no pct exec, no
        // pools/owners/routes, no `pct list` — for a dashboard page that
        // only wants to know what pve is actually enforcing right now.
        Rpc::GetFirewallLive { stack_files } => {
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            match store.load().await {
                Ok(snapshot) => {
                    let facts = homelab_core::ops::facts::gather_firewall_facts(
                        &exec,
                        &snapshot,
                        &state.config.safety.no_touch,
                    )
                    .await;
                    let statuses = homelab_core::ops::fleetcheck::firewall_live_status(
                        &snapshot,
                        &facts,
                        state.config.tile_watch_source.as_deref(),
                        &state.config.trusted_sources,
                    );
                    // Only what the caller named: the host may track stacks
                    // the dashboard's working copy does not (yet), and
                    // answering about those would be noise it never asked
                    // for.
                    let wanted: std::collections::HashSet<&str> =
                        stack_files.iter().map(|(s, _)| s.as_str()).collect();
                    let statuses: std::collections::BTreeMap<_, _> = statuses
                        .into_iter()
                        .filter(|(stack, _)| wanted.contains(stack.as_str()))
                        .collect();
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::json!({ "statuses": statuses }).to_string(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("state unreadable: {}", e),
                    deferred: None,
                },
            }
        }
        // feat-backup-1: per-repository status (D25's owning-app repos for
        // a compose stack, one per unit for a native stack), joined with
        // the restore-drill verdict the nightly drill already recorded.
        // fix-180: answered from `state.snapshot_cache`, same owner-keyed
        // cache `BackupCalendar` reads — a repository the two pages both
        // show is read from restic once, not once per page. A repository
        // never read before answers `repo_status_unread` ("not read yet")
        // at once, and a background refresh fills the cache for next time.
        Rpc::GetBackups {
            stack,
            force,
            source,
        } => {
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await.unwrap_or_default();
            let Some(st) = snapshot.stacks.get(&stack) else {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("no such stack '{}'", stack),
                    deferred: None,
                };
            };
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers,
                ..state.config.backup.clone()
            };
            // fix-218 round 2: the same owners doctor and the fleet check
            // read. Taking the manifest's mounts whenever one was recorded
            // dropped a native stack's unit repositories (inbox: manifest
            // with `storage: []` since fix-145 → "no repositories").
            let owners = homelab_core::ops::backup::stack_repo_owners(st);
            let local_available = state.config.backup.local_dataset.is_some();
            // restore-two-sources-1: the local set is read directly — it is
            // a disk away, so it needs none of the remote's cache.
            if source.as_deref() == Some("local") {
                let cfg = match backup_cfg_for_source(&exec, cfg, Some("local")).await {
                    Ok(c) => c,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: e,
                            deferred: None,
                        };
                    }
                };
                let mut statuses = Vec::with_capacity(owners.len());
                for owner in owners {
                    statuses.push(
                        homelab_core::ops::backup::repo_status_of(
                            &exec,
                            &cfg,
                            &snapshot.restore_drills,
                            owner,
                        )
                        .await,
                    );
                }
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::json!({
                        "native": st.is_native(),
                        "repos": statuses,
                        "source": "local",
                        "local_available": local_available,
                    })
                    .to_string(),
                    deferred: None,
                };
            }
            let mut statuses = Vec::with_capacity(owners.len());
            for owner in owners {
                match state.snapshot_cache.get(&owner) {
                    Some(cached) => {
                        if force {
                            kick_snapshot_refresh(state, cfg.clone(), owner.clone());
                        }
                        statuses.push(homelab_core::ops::backup::repo_status_from_cache(
                            &cached,
                            &snapshot.restore_drills,
                            owner,
                        ));
                    }
                    None => {
                        kick_snapshot_refresh(state, cfg.clone(), owner.clone());
                        statuses.push(homelab_core::ops::backup::repo_status_unread(
                            &snapshot.restore_drills,
                            owner,
                        ));
                    }
                }
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({
                    "native": st.is_native(),
                    "repos": statuses,
                    "source": "remote",
                    "local_available": local_available,
                })
                .to_string(),
                deferred: None,
            }
        }
        // feat-retired-1: every retired entry for the dashboard's Retired
        // page, each repository's snapshot count/size/newest filled in from
        // the same snapshot cache `GetBackups` reads (fix-180) — never a
        // fresh restic call, so this never blocks on Google Drive.
        Rpc::GetRetired => {
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await.unwrap_or_default();
            let rows = homelab_core::ops::retired::retired_rows(&snapshot, &state.config.state_dir);
            let out: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|r| {
                    let repos: Vec<serde_json::Value> = r
                        .repos
                        .iter()
                        .map(|repo| {
                            let cached = state.snapshot_cache.get(repo);
                            serde_json::json!({
                                "name": repo,
                                "in_use": r.in_use.contains(repo),
                                "snapshot_count": cached.as_ref().map(|c| c.snapshots.len()),
                                "size_bytes": cached.as_ref().and_then(|c| c.size_bytes),
                                "newest_snapshot": cached.as_ref().and_then(|c| c.snapshots.first().cloned()),
                                "measured_at": cached.as_ref().map(|c| c.measured_at),
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "key": r.key,
                        "kind": r.kind,
                        "stack": r.stack,
                        "name": r.name,
                        "vmid": r.vmid,
                        "retired_at": r.retired_at,
                        "repos": repos,
                        "appdata": r.appdata,
                        "vault": r.vault,
                        "in_use": r.in_use,
                        "refused": r.refused,
                    })
                })
                .collect();
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "retired": out }).to_string(),
                deferred: None,
            }
        }
        // feat-backup-3: read-only, never stops anything.
        Rpc::BrowseSnapshot {
            owner,
            snapshot,
            path,
            source,
        } => {
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let base = homelab_core::ops::backup::BackupCfg {
                tiers,
                ..state.config.backup.clone()
            };
            let cfg = match backup_cfg_for_source(&exec, base, source.as_deref()).await {
                Ok(c) => c,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    };
                }
            };
            match homelab_core::ops::backup::browse_snapshot(&exec, &cfg, &owner, &snapshot, &path)
                .await
            {
                Ok(entries) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e.to_string(),
                    deferred: None,
                },
            }
        }
        // fix-241: one file from a snapshot, read-only: `restic dump` to
        // stdout, capped; nothing restored, nothing written, nothing stopped.
        Rpc::ReadSnapshotFile {
            stack,
            owner,
            snapshot,
            path,
            source,
        } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let target = match store.load().await {
                Ok(hs) => {
                    homelab_core::ops::backup::snapshot_file_target(&hs, &stack, &owner, &path)
                }
                Err(e) => Err(format!("state unreadable: {}", e)),
            };
            let path = match target {
                Ok(p) => p,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    };
                }
            };
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let base = homelab_core::ops::backup::BackupCfg {
                tiers,
                ..state.config.backup.clone()
            };
            let cfg = match backup_cfg_for_source(&exec, base, source.as_deref()).await {
                Ok(c) => c,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    };
                }
            };
            match homelab_core::ops::backup::read_snapshot_file(
                &exec, &cfg, &owner, &snapshot, &path,
            )
            .await
            {
                Ok(file) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::to_string(&file).unwrap_or_else(|_| "{}".into()),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e.to_string(),
                    deferred: None,
                },
            }
        }
        // fix-237: one snapshot restored into the drill's scratch directory
        // with the drill's own restore-and-judge, counted, and emptied again;
        // the live data is never a target.
        Rpc::VerifyRestore {
            stack,
            app,
            snapshot,
            source,
        } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let owners = match store.load().await {
                Ok(hs) => homelab_core::ops::restoredrill::verify_restore_owners(
                    &hs,
                    &stack,
                    app.as_deref(),
                    &snapshot,
                ),
                Err(e) => Err(format!("state unreadable: {}", e)),
            };
            let owners = match owners {
                Ok(o) => o,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    };
                }
            };
            let cfg =
                match backup_cfg_for_source(&exec, state.config.backup.clone(), source.as_deref())
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: e,
                            deferred: None,
                        };
                    }
                };
            let scratch = state.config.restore_drill_scratch_dir.clone();
            run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "verify-restore",
                Some(&stack),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::restoredrill::verify_restore(
                            ctx, &cfg, &scratch, &owners, &snapshot,
                        )
                        .await
                    })
                },
            )
            .await
        }
        Rpc::RestoreNative {
            stack,
            snapshot,
            confirm,
            unit,
            source,
        } => match native_from_state(&state.config.state_dir, &stack).await {
            Ok((services, _)) => {
                // fix-223: a named unit restricts the restore to that one
                // unit's own repository; `None` (and an older client, which
                // never sends the field) restores every unit of the stack
                // from the same snapshot, exactly as before.
                let services = match &unit {
                    Some(u) => {
                        let matched: Vec<_> =
                            services.into_iter().filter(|m| &m.unit == u).collect();
                        if matched.is_empty() {
                            return RpcResponse {
                                id: req.id,
                                ok: false,
                                message: format!("stack '{}' has no unit named '{}'", stack, u),
                                deferred: None,
                            };
                        }
                        matched
                    }
                    None => services,
                };
                let cfg = match backup_cfg_for_source(
                    &exec,
                    state.config.backup.clone(),
                    source.as_deref(),
                )
                .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: e,
                            deferred: None,
                        };
                    }
                };
                let mut resp = RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!("no services on stack '{}'", stack),
                    deferred: None,
                };
                for m in services {
                    let cfg = cfg.clone();
                    let snap = snapshot.clone();
                    let confirm = confirm.clone();
                    let owner = m.unit.clone();
                    let r = run_mutating_op_for_stack(
                        state,
                        &exec,
                        req.id,
                        "restore-native",
                        Some(&stack),
                        |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::restore_native(
                                    ctx,
                                    &m,
                                    &cfg,
                                    &snap,
                                    confirm.as_deref(),
                                )
                                .await
                            })
                        },
                    )
                    .await;
                    let failed = !r.ok;
                    if r.ok {
                        kick_snapshot_refresh(state, state.config.backup.clone(), owner);
                    }
                    if resp.ok || failed {
                        resp = r;
                    }
                    if failed {
                        break;
                    }
                }
                resp
            }
            Err(msg) => RpcResponse {
                id: req.id,
                ok: false,
                message: msg,
                deferred: None,
            },
        },
        // feat-secrets-1: the value, read straight from the host's own
        // vault (never a process, never traced). Audited before the read,
        // naming what was asked for, never the value.
        //
        // redesign-3.71 secrets: and one history line (history.jsonl, what
        // the dashboard's Activity reads) naming who revealed or copied
        // which secret — `audit.by` when the dashboard names the person or
        // Live view, else the session's token name. Never the value.
        Rpc::RevealSecret {
            stack,
            secret,
            audit,
        } => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let purpose = audit.as_ref().map(|a| a.purpose).unwrap_or_default();
            let by = audit.map(|a| a.by).or_else(requested_by);
            let line = format!(
                "{} {} stack={} secret={:?} by={}\n",
                ts,
                purpose.label(),
                stack,
                secret,
                by.as_deref().unwrap_or("-")
            );
            if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &line) {
                tracing::warn!("audit.log: could not record the reveal :: {}", e);
            }
            let read = crate::secrets::reveal(&state.config.state_dir, &stack, &secret).await;
            record_history(
                state,
                &homelab_core::ops::secrets::reveal_history(
                    ts,
                    &stack,
                    &secret,
                    purpose,
                    by,
                    (req.id != 0).then_some(req.id),
                    read.as_ref()
                        .err()
                        .map(|_| "no sealed copy on the host".to_string()),
                ),
            );
            match read {
                Ok(content) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: content,
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // feat-secrets-2: through latch, exactly the one relative path a
        // deploy would read back — the rest of latch is untouched. Audited
        // before the write, never the value; `latch_put` itself never goes
        // through `Executor`/`Cmd` either (core::ops::secrets).
        Rpc::SetSecret {
            stack,
            secret,
            content,
        } => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let rel = homelab_core::ops::secrets::latch_rel_path(&stack, &secret);
            let audit = format!(
                "{} set-secret stack={} secret={:?} rel={}\n",
                ts, stack, secret, rel
            );
            if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &audit) {
                tracing::warn!("audit.log: could not record the write :: {}", e);
            }
            let env = std::env::var("HOMELAB_LATCH_ENV").unwrap_or_default();
            let project_root = format!("{}/repo", state.config.state_dir);
            let result = tokio::task::spawn_blocking(move || {
                crate::secrets::latch_put(&project_root, &env, &rel, &content)
            })
            .await;
            match result {
                Ok(Ok(())) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "{} written in latch; the other files latch holds for this stack are \
                         untouched; redeploy the stack for the running container to pick it up",
                        homelab_core::ops::secrets::latch_rel_path(&stack, &secret)
                    ),
                    deferred: None,
                },
                Ok(Err(e)) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("internal: {e}"),
                    deferred: None,
                },
            }
        }
        Rpc::GetConfig => {
            let view = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let _ = state.log_tx.send(ServerMsg::Config(Box::new(view)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "config".into(),
                deferred: None,
            }
        }
        Rpc::SetConfig(view) => {
            // Validate before persisting.
            if let Some(h) = view.backup_hour
                && h > 23
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "backup_hour must be 0-23".into(),
                    deferred: None,
                };
            }
            if view.retention.is_empty() {
                return RpcResponse {
                    id: req.id,
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
                        id: req.id,
                        ok: true,
                        message: "settings saved and applied".into(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("persist settings: {}", e),
                    deferred: None,
                },
            }
        }
        // feat-settings-1: read and write host.toml for the dashboard. The
        // answers go to the asking session only; nothing is broadcast, so a
        // TUI's settings screen keeps its unsaved edits.
        Rpc::GetHostConfig => {
            let path = &state.config.config_path;
            let raw = std::fs::read_to_string(path).unwrap_or_default();
            match host_config_view(path, &raw) {
                Ok(view) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::to_string(&view).unwrap_or_default(),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::SetHostConfig {
            changes,
            expect_sha256,
        } => {
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
                        id: req.id,
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
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&saved).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // fix-120 (per-machine tokens, owner decision 2026-10-01): mint,
        // list and revoke `[[tokens]]` entries. The scope-all audit line
        // above already named who issued or revoked.
        Rpc::TokenIssue { name, scope } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let plain = match random_token() {
                Ok(t) => t,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    };
                }
            };
            match issue_token_in(&raw, &name, scope, &plain)
                .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
            {
                Ok(tokens) => {
                    *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
                    info!(name = %name, scope = ?scope, "token issued");
                    let issued = homelab_proto::TokenIssued {
                        name,
                        scope,
                        token: plain,
                    };
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&issued).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::TokenList => {
            let mut views: Vec<homelab_proto::TokenView> = state
                .live_tokens()
                .into_iter()
                .map(|t| homelab_proto::TokenView {
                    name: t.name,
                    scope: t.scope,
                })
                .collect();
            if !state.config.token.is_empty() {
                views.insert(
                    0,
                    homelab_proto::TokenView {
                        name: "legacy".into(),
                        scope: homelab_proto::Scope::All,
                    },
                );
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&views).unwrap_or_default(),
                deferred: None,
            }
        }
        Rpc::TokenRevoke { name } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            match revoke_token_in(&raw, &name)
                .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
            {
                Ok(tokens) => {
                    *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
                    info!(name = %name, "token revoked");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("token {:?} revoked", name),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // fix-110: `config/host.toml` sent whole — `homelab host apply`, or
        // the dashboard's declarative commit of it. Same outcome shape as
        // `SetHostConfig`: the G8 keys go live immediately, the rest waits
        // for the host's next start.
        Rpc::ApplyHostConfig {
            toml: declared,
            expect_sha256,
            allow,
        } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let outcome =
                apply_host_config_whole(&raw, &declared, expect_sha256.as_deref(), &allow)
                    .and_then(|(text, saved)| {
                        write_config_file(&path, &text).map(|()| (text, saved))
                    });
            match outcome {
                Ok((text, saved)) => {
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
                        // fix-122: same live reload as `SetHostConfig`.
                        let _ = state.log_filter.reload(
                            tracing_subscriber::EnvFilter::try_new(&new_level)
                                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                        );
                    }
                    info!("host.toml applied from config/host.toml");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&saved).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::SelfUpdateHost { binary_b64, proof } => {
            use base64::Engine as _;
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&binary_b64) {
                Ok(b) => b,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
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
                    id: req.id,
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
                    id: req.id,
                    ok: false,
                    message: format!("stage binary: {}", e),
                    deferred: None,
                };
            }
            info!("self-update requested: staged {} bytes", bytes.len());
            run_mutating_op(state, &exec, req.id, "self-update", |ctx| {
                Box::pin(async move { homelab_core::ops::selfupdate::self_update(ctx, &cfg).await })
            })
            .await
        }
        // Owner decision 2026-09-30 (item 2): "Save and restart the host".
        // Refused outright rather than queued: waiting behind a running
        // job and then restarting mid-nightly-round is not what "restart
        // the host" should ever mean.
        Rpc::RestartHost => {
            if let Some(holder) = state.busy.lock().ok().and_then(|b| b.clone()) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "refused: an operation is running ({}); running jobs are refused while a job runs",
                        holder.what
                    ),
                    deferred: None,
                };
            }
            run_mutating_op(state, &exec, req.id, "restart-host", |ctx| {
                Box::pin(async move { homelab_core::ops::restarthost::restart_host(ctx).await })
            })
            .await
        }
        Rpc::Doctor { json } => {
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
            gather_security_probes(&exec, &ProbeContext::of(&state.config), now, &mut probes).await;
            let checks = homelab_core::doctor::diagnose(&probes);
            let overall = homelab_core::doctor::overall(&checks);
            if json {
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::json!({ "overall": overall, "checks": checks })
                        .to_string(),
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
                id: req.id,
                ok: overall != homelab_core::doctor::Health::Fail,
                message: msg,
                deferred: None,
            }
        }
        Rpc::GetState => {
            let fleet = build_fleet_state(state, &exec).await;
            let _ = state.log_tx.send(ServerMsg::State(Box::new(fleet)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "state".into(),
                deferred: None,
            }
        }
        // fix-131: one bundle, read on the workstation.
        Rpc::IncidentShow { name } => {
            let (ok, message) = match incident_text(&state.config.state_dir, &name) {
                Ok(text) => (true, text),
                Err(why) => (false, why),
            };
            RpcResponse {
                id: req.id,
                ok,
                message,
                deferred: None,
            }
        }
        Rpc::Incidents { json } => {
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
                id: req.id,
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
    }
}
