//! Homelab Client TUI — entry point and event loop.
//!
//! Runs on the Linux client desktop only (never on Proxmox).
//! GitOps-first: all changes go through Git; no direct SSH deployments.

use color_eyre::eyre::Result;
use crossterm::{
    cursor::{MoveTo, Show},
    event, execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

mod app;
mod app_list;
mod backup_schedule;
mod blast_radius;
mod events;
mod gitops;
mod latch;
mod opnsense;
mod scaffold;
mod shell;
mod ssh_config;
mod stack_features;
mod theme;
mod transactions;
mod ui;
mod ws_client;

use app::App;
use app::HostLxcRuntime;
use blast_radius::{ActiveModal, OperationEntry, OperationProgressState};
use ws_client::WsEvent;

enum SyncEvent {
    Accepted {
        stack: String,
    },
    LiveLog {
        stack: String,
        line: String,
    },
    Finished {
        stack: String,
        ok: bool,
        msg: String,
    },
}

#[derive(Serialize)]
struct RestoreRequestPayload {
    scope: String,
    stack_names: Vec<String>,
    backup_id: String,
    verify_only: bool,
    skip_post_hooks: bool,
}

#[derive(Deserialize)]
struct RestoreEventPayload {
    stack_name: String,
    phase: String,
    message: String,
    is_error: bool,
}

#[derive(Deserialize)]
struct RestoreStatusPayload {
    events: Vec<RestoreEventPayload>,
    success: bool,
    error_message: Option<String>,
}

#[derive(Deserialize)]
struct WsRestoreResponsePayload {
    ok: bool,
    status: RestoreStatusPayload,
    message: String,
}

enum RestoreDispatchEvent {
    Finished {
        stack: String,
        payload: Option<RestoreStatusPayload>,
        ok: bool,
        msg: String,
    },
}

enum HostProbeEvent {
    Snapshot {
        connected: bool,
        node_name: Option<String>,
        node_ip: Option<String>,
        uptime: Option<String>,
        lxc_runtime: Vec<HostLxcRuntime>,
        error: Option<String>,
    },
}

enum HostVersionEvent {
    Snapshot {
        connected: bool,
        version: Option<String>,
        latch_version: Option<String>,
        error: Option<String>,
    },
}

#[derive(Deserialize)]
struct HostMetricsResponse {
    hostname: String,
    ip: String,
    uptime_secs: u64,
    lxc_runtime: Vec<HostLxcRuntime>,
}

#[derive(Debug, Deserialize)]
struct HostVersionResponse {
    component: String,
    version: String,
    latch_version: Option<String>,
}

enum UpdateDispatchEvent {
    Finished {
        target: String,
        ok: bool,
        msg: String,
    },
}

enum UpdateCatalogEvent {
    HostLatest {
        latest_release: String,
        checked_at: String,
    },
}

const PROVISION_DISPATCH_DEBOUNCE_SECS: u64 = 8;

/// Bootstraps the Tokio runtime and hands off to the async event loop.
fn main() -> Result<()> {
    color_eyre::install()?;

    // Restore the terminal even if a panic occurs
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));

    Runtime::new()?.block_on(async_main())
}

fn load_client_env() {
    let candidates = [
        std::env::var("CLIENT_ENV_FILE").ok(),
        Some("config/.env".to_string()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = Path::new(&candidate);
        if path.exists() {
            let _ = dotenvy::from_path(path);
            break;
        }
    }
}

/// Core event loop: draw → wait for input → handle → repeat.
async fn async_main() -> Result<()> {
    // Change to project root so relative paths work correctly
    if let Ok(mut current) = std::env::current_dir() {
        for _ in 0..10 {
            if current.join(".git").exists() {
                std::env::set_current_dir(&current)?;
                break;
            }
            if !current.pop() {
                break;
            }
        }
    }

    load_client_env();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Channel for background tasks (sync HTTP calls) to report results back to the TUI.
    let (sync_tx, mut sync_rx) = mpsc::unbounded_channel::<SyncEvent>();
    let (restore_tx, mut restore_rx) = mpsc::unbounded_channel::<RestoreDispatchEvent>();
    let (update_tx, mut update_rx) = mpsc::unbounded_channel::<UpdateDispatchEvent>();
    let (update_catalog_tx, mut update_catalog_rx) =
        mpsc::unbounded_channel::<UpdateCatalogEvent>();
    // WebSocket event channel: receives continuous log streams from HOST and LXC stacks.
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<WsEvent>();
    let (host_probe_tx, mut host_probe_rx) = mpsc::unbounded_channel::<HostProbeEvent>();
    let (host_version_tx, mut host_version_rx) = mpsc::unbounded_channel::<HostVersionEvent>();

    let sigint = signal::ctrl_c();
    tokio::pin!(sigint);

    // Drives all visual animations: pulse, ticker, sparklines. Target ~30 FPS.
    let mut anim_tick = tokio::time::interval(Duration::from_millis(33));
    // While CLIENT is active, periodically pulse heartbeats so LXC can suppress failsafe pulls.
    let mut heartbeat_tick = tokio::time::interval(Duration::from_secs(30));
    // Reconcile websocket workers against active stacks.
    let mut ws_reconcile_tick = tokio::time::interval(Duration::from_secs(5));
    // Refresh host runtime telemetry for Host Management tab.
    let mut host_probe_tick = tokio::time::interval(Duration::from_secs(15));
    // Refresh available update metadata shown in Update cards.
    let mut update_catalog_tick = tokio::time::interval(Duration::from_secs(900));

    let mut lxc_ws_tasks: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut last_provision_dispatch: HashMap<String, Instant> = HashMap::new();
    let mut update_dispatch_running = false;

    // Always keep HOST websocket connected while CLIENT is running.
    let host_ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    app.push_log(
        "HOST",
        "INFO",
        &format!(
            "HOST handshake: connecting websocket ws://{}:8080/api/logs/ws",
            host_ip
        ),
    );
    let ws_tx_host = ws_tx.clone();
    tokio::spawn(async move {
        ws_client::connect_host_logs(&host_ip, 8080, ws_tx_host).await;
    });

    {
        let tx = host_version_tx.clone();
        tokio::spawn(async move {
            let snapshot = probe_host_version().await;
            let _ = tx.send(snapshot);
        });
    }

    for (stack, ip) in active_stack_targets(&app.stacks) {
        let stack_clone = stack.clone();
        let ip_clone = ip.clone();
        let ws_tx_lxc = ws_tx.clone();
        let handle = tokio::spawn(async move {
            ws_client::connect_lxc_logs(&stack_clone, &ip_clone, 8080, ws_tx_lxc).await;
        });
        lxc_ws_tasks.insert(stack, handle);
    }

    {
        let tx = host_probe_tx.clone();
        tokio::spawn(async move {
            let snapshot = probe_host_runtime().await;
            let _ = tx.send(snapshot);
        });
    }

    {
        let tx = update_catalog_tx.clone();
        tokio::spawn(async move {
            let latest = fetch_latest_host_release_tag()
                .await
                .unwrap_or_else(|e| format!("unavailable ({})", e));
            let _ = tx.send(UpdateCatalogEvent::HostLatest {
                latest_release: latest,
                checked_at: current_hms(),
            });
        });
    }

    loop {
        // Drain any sync results that arrived from background tasks
        while let Ok(event) = sync_rx.try_recv() {
            match event {
                SyncEvent::Accepted { stack } => {
                    app.push_client_logfmt(
                        "INFO",
                        Some(&stack),
                        Some("sync_result"),
                        "Sync accepted by LXC daemon",
                        None,
                    );
                    app.sync_status = format!("Sync accepted — '{}'", stack);
                }
                SyncEvent::LiveLog { stack, line } => {
                    app.mark_live_logs_seen();
                    let source = format!("lxc-{}", stack);
                    let (level, message) = parse_live_log_line(&line);
                    app.push_log(&source, &level, &message);
                }
                SyncEvent::Finished { stack, ok, msg } => {
                    let level = if ok { "INFO" } else { "ERROR" };
                    app.push_client_logfmt(
                        level,
                        Some(&stack),
                        Some("sync_result"),
                        &msg,
                        if ok { None } else { Some(&msg) },
                    );
                    app.sync_status = if ok {
                        format!("Sync OK — '{}'", stack)
                    } else {
                        format!("Sync failed — {} : {}", stack, msg)
                    };

                    if ok {
                        let summary = crate::stack_features::post_deploy_summary(&stack);
                        if summary.missing_compose.is_empty() {
                            app.push_client_logfmt(
                                "INFO",
                                Some(&stack),
                                Some("post_deploy"),
                                &format!(
                                    "post-deploy validation complete apps_healthy={}",
                                    summary.app_count
                                ),
                                None,
                            );
                        } else {
                            app.push_client_logfmt(
                                "WARN",
                                Some(&stack),
                                Some("post_deploy"),
                                &format!(
                                    "post-deploy validation warnings apps_missing_compose={}",
                                    summary.missing_compose.join(",")
                                ),
                                None,
                            );
                        }
                    }
                }
            }
        }

        while let Ok(event) = restore_rx.try_recv() {
            match event {
                RestoreDispatchEvent::Finished {
                    stack,
                    payload,
                    ok,
                    msg,
                } => {
                    let level = if ok { "INFO" } else { "ERROR" };
                    app.push_client_logfmt(
                        level,
                        Some(&stack),
                        Some("restore_result"),
                        &msg,
                        if ok { None } else { Some(&msg) },
                    );

                    if let Some(body) = payload {
                        let entries: Vec<OperationEntry> = if body.events.is_empty() {
                            vec![OperationEntry {
                                name: stack.clone(),
                                status: if ok { "✓" } else { "✗" }.to_string(),
                                detail: msg.clone(),
                            }]
                        } else {
                            body.events
                                .into_iter()
                                .map(|event| OperationEntry {
                                    name: event.stack_name,
                                    status: if event.is_error { "✗" } else { "✓" }.to_string(),
                                    detail: format!("{}: {}", event.phase, event.message),
                                })
                                .collect()
                        };

                        app.modal = ActiveModal::OperationProgress(OperationProgressState {
                            title: format!("Restore Result - {}", stack),
                            phase: if body.success {
                                "Completed".to_string()
                            } else {
                                "Failed".to_string()
                            },
                            summary: body.error_message.unwrap_or_else(|| msg.clone()),
                            entries,
                        });
                    }

                    app.backup_status = if ok {
                        format!("restore completed for '{}'", stack)
                    } else {
                        format!("restore failed for '{}': {}", stack, msg)
                    };
                }
            }
        }

        while let Ok(event) = ws_rx.try_recv() {
            match event {
                WsEvent::LogMessage { source, line } => {
                    // Parse log level from the line (e.g., "[INFO] message" or "message")
                    let (level, message) = if let Some(stripped) = line.strip_prefix("[INFO] ") {
                        ("INFO".to_string(), stripped.to_string())
                    } else if let Some(stripped) = line.strip_prefix("[ERROR] ") {
                        ("ERROR".to_string(), stripped.to_string())
                    } else if let Some(stripped) = line.strip_prefix("[WARN] ") {
                        ("WARN".to_string(), stripped.to_string())
                    } else if let Some(stripped) = line.strip_prefix("[OK] ") {
                        ("OK".to_string(), stripped.to_string())
                    } else {
                        ("INFO".to_string(), line.clone())
                    };

                    app.push_log(&source, &level, &message);
                    track_daemon_version(&mut app, &source, &message);
                }
                WsEvent::ConnectionStateChanged {
                    source,
                    connected,
                    error,
                } => {
                    if source == "HOST" {
                        app.host_connected = connected;
                        if connected {
                            let tx = host_version_tx.clone();
                            tokio::spawn(async move {
                                let snapshot = probe_host_version().await;
                                let _ = tx.send(snapshot);
                            });
                        }
                        let msg = if connected {
                            "HOST WebSocket connected".to_string()
                        } else {
                            format!(
                                "HOST WebSocket disconnected: {}",
                                error.unwrap_or_else(|| "unknown".to_string())
                            )
                        };
                        app.push_log("HOST", if connected { "INFO" } else { "WARN" }, &msg);
                    } else {
                        app.set_lxc_source_connected(&source, connected);
                        let msg = if connected {
                            format!("{} WebSocket connected", source)
                        } else {
                            format!(
                                "{} WebSocket disconnected: {}",
                                source,
                                error.unwrap_or_else(|| "unknown".to_string())
                            )
                        };
                        app.push_log(&source, if connected { "INFO" } else { "WARN" }, &msg);
                    }
                }
            }
        }

        while let Ok(event) = host_version_rx.try_recv() {
            match event {
                HostVersionEvent::Snapshot {
                    connected,
                    version,
                    latch_version,
                    error,
                } => {
                    if connected {
                        let daemon = version.unwrap_or_else(|| "unknown".to_string());
                        let latch = latch_version.unwrap_or_else(|| "unknown".to_string());
                        let msg = format!(
                            "HOST announce daemon_version={} latch_version={}",
                            daemon, latch
                        );
                        app.push_log("HOST", "INFO", &msg);
                        track_daemon_version(&mut app, "HOST", &msg);
                    } else if let Some(err) = error {
                        app.push_log(
                            "HOST",
                            "WARN",
                            &format!("HOST version probe failed: {}", err),
                        );
                    }
                }
            }
        }

        while let Ok(event) = host_probe_rx.try_recv() {
            match event {
                HostProbeEvent::Snapshot {
                    connected,
                    node_name,
                    node_ip,
                    uptime,
                    lxc_runtime,
                    error,
                } => {
                    app.update_host_runtime(
                        connected,
                        node_name,
                        node_ip,
                        uptime,
                        lxc_runtime,
                        error,
                    );
                }
            }
        }

        while let Ok(event) = update_rx.try_recv() {
            match event {
                UpdateDispatchEvent::Finished { target, ok, msg } => {
                    update_dispatch_running = false;
                    app.update_in_progress = None;
                    app.record_update_result(&target, ok, &msg);
                    app.update_status = if ok {
                        format!("{} update success: {}", target, msg)
                    } else {
                        format!("{} update failed: {}", target, msg)
                    };
                    app.push_client_logfmt(
                        if ok { "INFO" } else { "ERROR" },
                        Some(&target),
                        Some("update_result"),
                        &msg,
                        if ok { None } else { Some(&msg) },
                    );
                }
            }
        }

        while let Ok(event) = update_catalog_rx.try_recv() {
            match event {
                UpdateCatalogEvent::HostLatest {
                    latest_release,
                    checked_at,
                } => {
                    app.host_latest_release = latest_release;
                    app.host_latest_checked_at = checked_at;
                }
            }
        }

        // Promote queued batch sync work into the single-sync execution slot.
        if !app.sync_pending {
            if let Some(next_stack) = app.sync_queue.pop_front() {
                app.sync_stack = next_stack;
                app.sync_pending = true;
            }
        }

        if !app.restore_pending {
            if let Some(next_stack) = app.restore_queue.pop_front() {
                app.restore_stack = next_stack;
                app.restore_pending = true;
            }
        }

        // If a provision was requested, ask HOST to create/reconcile LXC containers.
        // This fires before the sync so the container exists when sync is attempted.
        if app.provision_pending {
            app.provision_pending = false;
            let stack = app.sync_stack.clone();

            let should_dispatch = match last_provision_dispatch.get(&stack) {
                Some(last) => last.elapsed().as_secs() >= PROVISION_DISPATCH_DEBOUNCE_SECS,
                None => true,
            };

            if !should_dispatch {
                app.push_client_logfmt(
                    "INFO",
                    Some(&stack),
                    Some("provision_dispatch"),
                    "duplicate HOST provision request suppressed by debounce",
                    None,
                );
            } else {
                last_provision_dispatch.insert(stack.clone(), Instant::now());

                let tx = sync_tx.clone();
                app.push_client_logfmt(
                    "INFO",
                    Some(&stack),
                    Some("provision_dispatch"),
                    "requesting HOST to provision LXC container",
                    None,
                );
                tokio::spawn(async move {
                    match trigger_host_provision(&stack).await {
                        Ok(msg) => {
                            let _ = tx.send(SyncEvent::LiveLog {
                            stack: stack.clone(),
                            line: format!(
                                "CLIENT INFO component=client level=info stack={} phase=provision_result msg=\"{}\"",
                                stack,
                                msg.replace('"', "'")
                            ),
                        });
                        }
                        Err(e) => {
                            let _ = tx.send(SyncEvent::LiveLog {
                            stack: stack.clone(),
                            line: format!(
                                "CLIENT ERROR component=client level=error stack={} phase=provision_result msg=\"HOST provision failed\" error=\"{}\"",
                                stack,
                                e.replace('"', "'")
                            ),
                        });
                        }
                    }
                });
            }
        }

        // If a stack-destroy was requested, ask HOST to destroy the selected LXC.
        if app.destroy_stack_pending {
            app.destroy_stack_pending = false;
            let stack = app.destroy_stack.clone();
            let tx = sync_tx.clone();
            app.push_client_logfmt(
                "WARN",
                Some(&stack),
                Some("destroy_dispatch"),
                "requesting HOST to destroy stack LXC container",
                None,
            );
            tokio::spawn(async move {
                match trigger_host_destroy_stack(&stack).await {
                    Ok(msg) => {
                        let _ = tx.send(SyncEvent::LiveLog {
                            stack: stack.clone(),
                            line: format!(
                                "CLIENT WARN component=client level=warn stack={} phase=destroy_result msg=\"{}\"",
                                stack,
                                msg.replace('"', "'")
                            ),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(SyncEvent::LiveLog {
                            stack: stack.clone(),
                            line: format!(
                                "CLIENT ERROR component=client level=error stack={} phase=destroy_result msg=\"HOST destroy failed\" error=\"{}\"",
                                stack,
                                e.replace('"', "'")
                            ),
                        });
                    }
                }
            });
        }

        // If a sync was queued by key actions, spawn the HTTP request now.
        if app.sync_pending {
            app.sync_pending = false;
            let stack = app.sync_stack.clone();
            let tx = sync_tx.clone();

            // Resolve LXC IP: prefer stacks/<name>/lxc-compose.yml reserved_ipv4,
            // then fall back to LXC_API_IP env var, then 127.0.0.1.
            let ip = resolve_stack_api_ip(&stack);
            let url = format!("http://{}:8080/api/sync", ip);
            let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
            app.push_client_logfmt(
                "INFO",
                Some(&stack),
                Some("sync_dispatch"),
                &format!("WS sync_request -> {}:8080", ip),
                None,
            );

            tokio::spawn(async move {
                if request_lxc_sync_ws(&ip, &token).await.is_ok() {
                    let _ = tx.send(SyncEvent::Accepted {
                        stack: stack.clone(),
                    });

                    let ws_url = format!("ws://{}:8080/api/logs/ws", ip);
                    if let Ok((mut socket, _)) = connect_async(&ws_url).await {
                        let mut streamed_any = false;
                        loop {
                            match tokio::time::timeout(Duration::from_secs(2), socket.next()).await
                            {
                                Ok(Some(Ok(message))) => {
                                    if let Ok(text) = message.into_text() {
                                        streamed_any = true;
                                        let _ = tx.send(SyncEvent::LiveLog {
                                            stack: stack.clone(),
                                            line: text,
                                        });
                                    }
                                }
                                Ok(Some(Err(err))) => {
                                    let _ = tx.send(SyncEvent::Finished {
                                        stack,
                                        ok: false,
                                        msg: format!("Live log stream failed: {}", err),
                                    });
                                    return;
                                }
                                Ok(None) => break,
                                Err(_) if streamed_any => break,
                                Err(_) => break,
                            }
                        }
                    }

                    let _ = tx.send(SyncEvent::Finished {
                        stack,
                        ok: true,
                        msg: "Sync completed via WebSocket RPC; live logs streamed to CLIENT"
                            .to_string(),
                    });
                    return;
                }

                let client = reqwest::Client::new();
                let mut req = client.post(&url);
                if !token.is_empty() {
                    req = req.bearer_auth(&token);
                }
                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let _ = tx.send(SyncEvent::Accepted {
                            stack: stack.clone(),
                        });

                        let ws_url = format!("ws://{}:8080/api/logs/ws", ip);
                        if let Ok((mut socket, _)) = connect_async(&ws_url).await {
                            let mut streamed_any = false;
                            loop {
                                match tokio::time::timeout(Duration::from_secs(2), socket.next())
                                    .await
                                {
                                    Ok(Some(Ok(message))) => {
                                        if let Ok(text) = message.into_text() {
                                            streamed_any = true;
                                            let _ = tx.send(SyncEvent::LiveLog {
                                                stack: stack.clone(),
                                                line: text,
                                            });
                                        }
                                    }
                                    Ok(Some(Err(err))) => {
                                        let _ = tx.send(SyncEvent::Finished {
                                            stack,
                                            ok: false,
                                            msg: format!("Live log stream failed: {}", err),
                                        });
                                        return;
                                    }
                                    Ok(None) => break,
                                    Err(_) if streamed_any => break,
                                    Err(_) => break,
                                }
                            }
                        }

                        let _ = tx.send(SyncEvent::Finished {
                            stack,
                            ok: true,
                            msg: "Sync completed; live logs streamed to CLIENT".to_string(),
                        });
                    }
                    Ok(resp) => {
                        let _ = tx.send(SyncEvent::Finished {
                            stack,
                            ok: false,
                            msg: format!("HTTP {}", resp.status()),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(SyncEvent::Finished {
                            stack,
                            ok: false,
                            msg: e.to_string(),
                        });
                    }
                }
            });
        }

        if app.restore_pending {
            app.restore_pending = false;
            let stack = app.restore_stack.clone();
            let backup_id = app.restore_backup_id.clone();
            let tx = restore_tx.clone();

            let ip = resolve_stack_api_ip(&stack);

            let url = format!("http://{}:8080/api/restore", ip);
            let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
            app.push_client_logfmt(
                "INFO",
                Some(&stack),
                Some("restore_dispatch"),
                &format!("WS restore_request -> {}:8080 backup_id={}", ip, backup_id),
                None,
            );

            tokio::spawn(async move {
                let payload = RestoreRequestPayload {
                    scope: "Stack".to_string(),
                    stack_names: vec![stack.clone()],
                    backup_id,
                    verify_only: false,
                    skip_post_hooks: false,
                };

                if let Ok(ws_body) = request_lxc_restore_ws(&ip, &token, &payload).await {
                    let msg = if ws_body.ok {
                        ws_body.message
                    } else {
                        ws_body
                            .status
                            .error_message
                            .clone()
                            .unwrap_or(ws_body.message)
                    };
                    let _ = tx.send(RestoreDispatchEvent::Finished {
                        stack,
                        payload: Some(ws_body.status),
                        ok: ws_body.ok,
                        msg,
                    });
                    return;
                }

                let client = reqwest::Client::new();
                let mut req = client.post(&url).json(&payload);
                if !token.is_empty() {
                    req = req.bearer_auth(&token);
                }

                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<RestoreStatusPayload>().await {
                            Ok(body) => {
                                let msg = if body.success {
                                    "Restore completed".to_string()
                                } else {
                                    body.error_message
                                        .clone()
                                        .unwrap_or_else(|| "Restore failed".to_string())
                                };
                                let _ = tx.send(RestoreDispatchEvent::Finished {
                                    stack,
                                    payload: Some(body),
                                    ok: true,
                                    msg,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(RestoreDispatchEvent::Finished {
                                    stack,
                                    payload: None,
                                    ok: false,
                                    msg: format!("Restore response parse failed: {}", e),
                                });
                            }
                        }
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.json::<RestoreStatusPayload>().await.ok();
                        let msg = body
                            .as_ref()
                            .and_then(|b| b.error_message.clone())
                            .unwrap_or_else(|| format!("HTTP {}", status));
                        let _ = tx.send(RestoreDispatchEvent::Finished {
                            stack,
                            payload: body,
                            ok: false,
                            msg,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(RestoreDispatchEvent::Finished {
                            stack,
                            payload: None,
                            ok: false,
                            msg: e.to_string(),
                        });
                    }
                }
            });
        }

        if !update_dispatch_running {
            if let Some(target) = app.update_in_progress.clone() {
                update_dispatch_running = true;
                let tx = update_tx.clone();
                let stacks = app.stacks.clone();
                let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();

                tokio::spawn(async move {
                    if target == "UPDATING_ALL" {
                        let mut ok = true;
                        let mut parts: Vec<String> = Vec::new();

                        let host_result = trigger_host_update().await;
                        match host_result {
                            Ok(msg) => parts.push(format!("HOST: {}", msg)),
                            Err(err) => {
                                ok = false;
                                parts.push(format!("HOST: {}", err));
                            }
                        }

                        for stack in stacks {
                            let ip = resolve_stack_api_ip(&stack);
                            match trigger_lxc_update(&ip, &token).await {
                                Ok(msg) => parts.push(format!("{}: {}", stack, msg)),
                                Err(err) => {
                                    ok = false;
                                    parts.push(format!("{}: {}", stack, err));
                                }
                            }
                        }

                        let _ = tx.send(UpdateDispatchEvent::Finished {
                            target: "UPDATING_ALL".to_string(),
                            ok,
                            msg: parts.join(" | "),
                        });
                        return;
                    }

                    if target == "HOST" {
                        let result = trigger_host_update().await;
                        let (ok, msg) = match result {
                            Ok(msg) => (true, msg),
                            Err(err) => (false, err),
                        };
                        let _ = tx.send(UpdateDispatchEvent::Finished { target, ok, msg });
                        return;
                    }

                    let ip = resolve_stack_api_ip(&target);
                    let result = trigger_lxc_update(&ip, &token).await;
                    let (ok, msg) = match result {
                        Ok(msg) => (true, msg),
                        Err(err) => (false, err),
                    };
                    let _ = tx.send(UpdateDispatchEvent::Finished { target, ok, msg });
                });
            }
        }

        // Always draw before polling — keeps the UI responsive
        terminal.draw(|f| ui::draw_ui(f, &app))?;

        tokio::select! {
            _ = &mut sigint => {
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;
                execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
                return Ok(());
            }

            _ = anim_tick.tick() => {
                app.tick_anim();
            }

            _ = heartbeat_tick.tick() => {
                let stacks = app.stacks.clone();
                let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
                tokio::spawn(async move {
                    send_heartbeats(stacks.clone(), token).await;
                    send_host_heartbeat(stacks).await;
                });
            }

            _ = ws_reconcile_tick.tick() => {
                let active_targets = active_stack_targets(&app.stacks);
                let active_set: HashSet<String> = active_targets.iter().map(|(stack, _)| stack.clone()).collect();

                for (stack, ip) in active_targets {
                    if lxc_ws_tasks.contains_key(&stack) {
                        continue;
                    }
                    let ws_tx_lxc = ws_tx.clone();
                    let stack_clone = stack.clone();
                    let ip_clone = ip.clone();
                    let handle = tokio::spawn(async move {
                        ws_client::connect_lxc_logs(&stack_clone, &ip_clone, 8080, ws_tx_lxc).await;
                    });
                    lxc_ws_tasks.insert(stack.clone(), handle);
                    app.push_client_logfmt(
                        "INFO",
                        Some(&stack),
                        Some("ws_reconcile"),
                        "started LXC websocket worker for active stack",
                        None,
                    );
                }

                let stale: Vec<String> = lxc_ws_tasks
                    .keys()
                    .filter(|stack| !active_set.contains(*stack))
                    .cloned()
                    .collect();

                for stack in stale {
                    if let Some(handle) = lxc_ws_tasks.remove(&stack) {
                        handle.abort();
                        app.push_client_logfmt(
                            "INFO",
                            Some(&stack),
                            Some("ws_reconcile"),
                            "stopped LXC websocket worker because stack is inactive",
                            None,
                        );
                    }
                }
            }

            _ = host_probe_tick.tick() => {
                let tx = host_probe_tx.clone();
                tokio::spawn(async move {
                    let snapshot = probe_host_runtime().await;
                    let _ = tx.send(snapshot);
                });
            }

            _ = update_catalog_tick.tick() => {
                let tx = update_catalog_tx.clone();
                tokio::spawn(async move {
                    let latest = fetch_latest_host_release_tag()
                        .await
                        .unwrap_or_else(|e| format!("unavailable ({})", e));
                    let _ = tx.send(UpdateCatalogEvent::HostLatest {
                        latest_release: latest,
                        checked_at: current_hms(),
                    });
                });
            }

            res = async {
                if let Ok(true) = event::poll(std::time::Duration::from_millis(100)) {
                    if let Ok(event::Event::Key(key)) = event::read() {
                        match events::handle_key_event(&mut app, key) {
                            events::EventOutcome::Quit => {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "quit",
                                ));
                            }
                            events::EventOutcome::Reload => {
                                app.reload_stacks_and_dropdowns();

                                let active_targets = active_stack_targets(&app.stacks);
                                let active_set: HashSet<String> = active_targets.iter().map(|(stack, _)| stack.clone()).collect();

                                for (stack, ip) in active_targets {
                                    if lxc_ws_tasks.contains_key(&stack) {
                                        continue;
                                    }
                                    let ws_tx_lxc = ws_tx.clone();
                                    let stack_clone = stack.clone();
                                    let ip_clone = ip.clone();
                                    let handle = tokio::spawn(async move {
                                        ws_client::connect_lxc_logs(&stack_clone, &ip_clone, 8080, ws_tx_lxc).await;
                                    });
                                    lxc_ws_tasks.insert(stack, handle);
                                }

                                let stale: Vec<String> = lxc_ws_tasks
                                    .keys()
                                    .filter(|stack| !active_set.contains(*stack))
                                    .cloned()
                                    .collect();
                                for stack in stale {
                                    if let Some(handle) = lxc_ws_tasks.remove(&stack) {
                                        handle.abort();
                                    }
                                }
                            }
                            events::EventOutcome::Continue => {}
                        }
                    }
                }
                Ok(()) as std::io::Result<()>
            } => {
                if res.is_err() {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn current_hms() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

async fn fetch_latest_host_release_tag() -> Result<String, String> {
    let repo = std::env::var("HOST_UPDATE_REPO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "kennypassenier/homelab".to_string());
    let url = format!("https://api.github.com/repos/{}/releases", repo);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "homelab-client")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("github_http_{}", response.status()));
    }

    let payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    let tags = payload
        .as_array()
        .ok_or_else(|| "unexpected_release_payload".to_string())?
        .iter()
        .filter_map(|release| release.get("tag_name").and_then(|v| v.as_str()))
        .filter(|tag| tag.starts_with("host-daemon-v"));

    let latest = tags
        .max_by(|a, b| compare_host_tag_versions(a, b))
        .ok_or_else(|| "no_host_daemon_release".to_string())?;

    Ok(latest.to_string())
}

fn compare_host_tag_versions(a: &str, b: &str) -> Ordering {
    let parse = |tag: &str| -> Vec<u64> {
        tag.trim_start_matches("host-daemon-v")
            .split('.')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };

    let a_parts = parse(a);
    let b_parts = parse(b);
    let len = a_parts.len().max(b_parts.len());

    for idx in 0..len {
        let av = a_parts.get(idx).copied().unwrap_or(0);
        let bv = b_parts.get(idx).copied().unwrap_or(0);
        match av.cmp(&bv) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

fn parse_live_log_line(line: &str) -> (String, String) {
    let level = line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("level="))
        .map(|value| value.trim_matches('"').to_uppercase())
        .unwrap_or_else(|| "INFO".to_string());
    let message = line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("msg="))
        .map(|value| value.trim_matches('"').replace('"', ""))
        .unwrap_or_else(|| line.to_string());
    (level, message)
}

fn resolve_stack_api_ip(stack: &str) -> String {
    if let Ok(config) = crate::scaffold::read_stack_config(stack) {
        if let Some(ip) = config.reserved_ipv4.filter(|ip| !ip.trim().is_empty()) {
            return ip;
        }
    }

    std::env::var("LXC_API_IP").unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn active_stack_targets(stacks: &[String]) -> Vec<(String, String)> {
    let mut targets = Vec::new();

    for stack in stacks {
        let cfg = match crate::scaffold::read_stack_config(stack) {
            Ok(cfg) => cfg,
            Err(_) => continue,
        };

        if !cfg.deploy_enabled {
            continue;
        }

        targets.push((stack.clone(), resolve_stack_api_ip(stack)));
    }

    targets
}

async fn send_heartbeats(stacks: Vec<String>, token: String) {
    let mut targets = HashSet::new();
    for stack in stacks {
        let cfg = match crate::scaffold::read_stack_config(&stack) {
            Ok(cfg) => cfg,
            Err(_) => continue,
        };

        if !cfg.deploy_enabled {
            continue;
        }

        let ip = resolve_stack_api_ip(&stack);
        if !ip.trim().is_empty() {
            targets.insert(ip);
        }
    }

    if targets.is_empty() {
        return;
    }

    for ip in targets {
        if request_lxc_heartbeat_ws(&ip, &token).await.is_err() {
            let client = reqwest::Client::new();
            let url = format!("http://{}:8080/api/heartbeat", ip);
            let mut req = client.post(url);
            if !token.is_empty() {
                req = req.bearer_auth(&token);
            }
            let _ = req.send().await;
        }
    }
}

async fn request_lxc_sync_ws(ip: &str, token: &str) -> Result<(), String> {
    let request_id = ws_request_id("sync");
    let latch = client_latch_pull_payload().unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "kind": "sync_request",
        "request_id": request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "latch": latch,
    });

    let value = send_ws_rpc(ip, payload, "sync_response", &request_id).await?;
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("sync rejected")
            .to_string())
    }
}

async fn request_lxc_heartbeat_ws(ip: &str, token: &str) -> Result<(), String> {
    let request_id = ws_request_id("heartbeat");
    let latch = client_latch_pull_payload().unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "kind": "heartbeat_request",
        "request_id": request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "latch": latch,
    });

    let value = send_ws_rpc(ip, payload, "heartbeat_response", &request_id).await?;
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("heartbeat rejected")
            .to_string())
    }
}

async fn request_lxc_restore_ws(
    ip: &str,
    token: &str,
    payload: &RestoreRequestPayload,
) -> Result<WsRestoreResponsePayload, String> {
    let request_id = ws_request_id("restore");
    let frame = serde_json::json!({
        "kind": "restore_request",
        "request_id": request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "scope": &payload.scope,
        "stack_names": &payload.stack_names,
        "backup_id": &payload.backup_id,
        "verify_only": payload.verify_only,
        "skip_post_hooks": payload.skip_post_hooks
    });

    let value = send_ws_rpc(ip, frame, "restore_response", &request_id).await?;
    serde_json::from_value::<WsRestoreResponsePayload>(value).map_err(|e| e.to_string())
}

async fn send_ws_rpc(
    ip: &str,
    payload: serde_json::Value,
    expected_kind: &str,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    let ws_url = format!("ws://{}:8080/api/logs/ws", ip);
    let (mut socket, _) = connect_async(&ws_url).await.map_err(|e| e.to_string())?;
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&payload).map_err(|e| e.to_string())?,
        ))
        .await
        .map_err(|e| e.to_string())?;

    loop {
        let next = tokio::time::timeout(Duration::from_secs(25), socket.next())
            .await
            .map_err(|_| "ws rpc timeout".to_string())?;
        let Some(message) = next else {
            return Err("ws rpc connection closed".to_string());
        };
        let message = message.map_err(|e| e.to_string())?;
        let text = match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => text,
            tokio_tungstenite::tungstenite::Message::Close(_) => {
                return Err("ws rpc connection closed".to_string());
            }
            _ => continue,
        };

        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let kind = value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let rid = value
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if kind == expected_kind && rid == request_id {
            return Ok(value);
        }
    }
}

fn ws_request_id(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

async fn request_host_update_ws() -> Result<(), String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let request_id = ws_request_id("update");
    let latch = client_latch_pull_payload().unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "kind": "update_request",
        "request_id": &request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "latch": latch,
    });

    send_ws_rpc(&ip, payload, "update_response", &request_id).await?;
    Ok(())
}

async fn request_host_heartbeat_ws(active_stacks: &[String]) -> Result<(), String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let request_id = ws_request_id("host-heartbeat");
    let latch = client_latch_pull_payload().unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "kind": "client_heartbeat",
        "request_id": &request_id,
        "active_stacks": active_stacks,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "latch": latch,
    });

    let value = send_ws_rpc(&ip, payload, "client_heartbeat_response", &request_id).await?;
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("host heartbeat rejected")
            .to_string())
    }
}

async fn request_host_heartbeat_http(active_stacks: &[String]) -> Result<(), String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let url = format!("http://{}:8080/api/heartbeat", ip);
    let client = reqwest::Client::new();
    let mut req = client.post(url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }

    let response = req
        .json(&serde_json::json!({ "active_stacks": active_stacks }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HOST heartbeat HTTP {}", response.status()))
    }
}

async fn request_lxc_update_ws(ip: &str, token: &str) -> Result<(), String> {
    let request_id = ws_request_id("update");
    let latch = client_latch_pull_payload().unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "kind": "update_request",
        "request_id": &request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "latch": latch,
    });

    send_ws_rpc(ip, payload, "update_response", &request_id).await?;
    Ok(())
}

async fn request_host_update_http() -> Result<String, String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let url = format!("http://{}:8080/api/update", ip);
    let latch = client_latch_pull_payload();
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "latch": latch.unwrap_or(serde_json::Value::Null) }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("HOST update check started".to_string())
    } else {
        Err(format!("HOST update HTTP {}", response.status()))
    }
}

async fn request_lxc_update_http(ip: &str, token: &str) -> Result<String, String> {
    let url = format!("http://{}:8080/api/update", ip);
    let client = reqwest::Client::new();
    let mut req = client.post(url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let latch = client_latch_pull_payload();
    let response = req
        .json(&serde_json::json!({ "latch": latch.unwrap_or(serde_json::Value::Null) }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("LXC update started".to_string())
    } else {
        Err(format!("LXC update HTTP {}", response.status()))
    }
}

async fn trigger_host_update() -> Result<String, String> {
    if request_host_update_ws().await.is_ok() {
        Ok("HOST update check started via websocket".to_string())
    } else {
        request_host_update_http().await
    }
}

/// Ask HOST to run an LXC provisioning cycle immediately via WS RPC or HTTP fallback.
async fn trigger_host_provision(stack_name: &str) -> Result<String, String> {
    if request_host_provision_ws(stack_name).await.is_ok() {
        Ok("HOST provisioning cycle started via websocket".to_string())
    } else {
        request_host_provision_http(stack_name).await
    }
}

/// Ask HOST to destroy one stack container via WS RPC or HTTP fallback.
async fn trigger_host_destroy_stack(stack_name: &str) -> Result<String, String> {
    if request_host_destroy_stack_ws(stack_name).await.is_ok() {
        Ok("HOST stack destroy started via websocket".to_string())
    } else {
        request_host_destroy_stack_http(stack_name).await
    }
}

async fn request_host_provision_ws(stack_name: &str) -> Result<(), String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let request_id = ws_request_id("provision");
    let payload = serde_json::json!({
        "kind": "provision_request",
        "request_id": &request_id,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
        "active_stacks": [stack_name],
    });
    send_ws_rpc(&ip, payload, "provision_response", &request_id).await?;
    Ok(())
}

fn client_latch_pull_payload() -> Option<serde_json::Value> {
    let ctx = crate::latch::load_latch_pull_context()?;
    serde_json::to_value(ctx).ok()
}

async fn request_host_provision_http(stack_name: &str) -> Result<String, String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let url = format!("http://{}:8080/api/provision", ip);
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "active_stacks": [stack_name] }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("HOST provisioning cycle started".to_string())
    } else {
        Err(format!("HOST provision HTTP {}", response.status()))
    }
}

async fn request_host_destroy_stack_ws(stack_name: &str) -> Result<(), String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let request_id = ws_request_id("destroy-stack");
    let payload = serde_json::json!({
        "kind": "destroy_stack_request",
        "request_id": &request_id,
        "stack_name": stack_name,
        "token": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(token.to_string()) },
    });
    send_ws_rpc(&ip, payload, "destroy_stack_response", &request_id).await?;
    Ok(())
}

async fn request_host_destroy_stack_http(stack_name: &str) -> Result<String, String> {
    let ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let url = format!("http://{}:8080/api/provision/destroy", ip);
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "stack_name": stack_name }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("HOST stack destroy started".to_string())
    } else {
        Err(format!("HOST destroy HTTP {}", response.status()))
    }
}

async fn trigger_lxc_update(ip: &str, token: &str) -> Result<String, String> {
    if request_lxc_update_ws(ip, token).await.is_ok() {
        Ok(format!("LXC update started via websocket ({})", ip))
    } else {
        request_lxc_update_http(ip, token).await
    }
}

fn track_daemon_version(app: &mut App, source: &str, message: &str) {
    let Some(version) = extract_daemon_version(message) else {
        return;
    };

    if source == "HOST" {
        if app.host_daemon_version != version {
            let previous = app.host_daemon_version.clone();
            app.host_daemon_version = version.clone();
            if previous != "unknown" {
                app.push_log(
                    "HOST",
                    "OK",
                    &format!("HOST daemon version changed {} -> {}", previous, version),
                );
            } else {
                app.push_log(
                    "HOST",
                    "INFO",
                    &format!("HOST daemon version detected {}", version),
                );
            }
        }
        return;
    }

    if source.starts_with("lxc-") {
        let previous = app
            .lxc_daemon_versions
            .insert(source.to_string(), version.clone());
        match previous {
            Some(prev) if prev != version => {
                app.push_log(
                    source,
                    "OK",
                    &format!("LXC daemon version changed {} -> {}", prev, version),
                );
            }
            None => {
                app.push_log(
                    source,
                    "INFO",
                    &format!("LXC daemon version detected {}", version),
                );
            }
            _ => {}
        }
    }
}

fn extract_daemon_version(message: &str) -> Option<String> {
    message
        .split_whitespace()
        .find_map(|part| part.strip_prefix("daemon_version="))
        .map(|raw| raw.trim_matches('"').to_string())
}

async fn send_host_heartbeat(stacks: Vec<String>) {
    let active_stacks: Vec<String> = stacks
        .iter()
        .filter_map(|stack| {
            crate::scaffold::read_stack_config(stack)
                .ok()
                .filter(|cfg| cfg.deploy_enabled)
                .map(|_| stack.clone())
        })
        .collect();

    if request_host_heartbeat_ws(&active_stacks).await.is_err() {
        let _ = request_host_heartbeat_http(&active_stacks).await;
    }
}

async fn probe_host_runtime() -> HostProbeEvent {
    let host_ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let url = format!("http://{}:8080/api/metrics", host_ip);
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    if !token.trim().is_empty() {
        request = request.bearer_auth(token);
    }

    let response = tokio::time::timeout(Duration::from_secs(6), request.send()).await;

    match response {
        Ok(Ok(resp)) if resp.status().is_success() => {
            match resp.json::<HostMetricsResponse>().await {
                Ok(metrics) => HostProbeEvent::Snapshot {
                    connected: true,
                    node_name: Some(metrics.hostname),
                    node_ip: Some(metrics.ip),
                    uptime: Some(format_uptime(metrics.uptime_secs)),
                    lxc_runtime: metrics.lxc_runtime,
                    error: None,
                },
                Err(err) => HostProbeEvent::Snapshot {
                    connected: false,
                    node_name: None,
                    node_ip: None,
                    uptime: None,
                    lxc_runtime: Vec::new(),
                    error: Some(format!("metrics parse failed: {}", err)),
                },
            }
        }
        Ok(Ok(resp)) => HostProbeEvent::Snapshot {
            connected: false,
            node_name: None,
            node_ip: None,
            uptime: None,
            lxc_runtime: Vec::new(),
            error: Some(format!("metrics HTTP {}", resp.status())),
        },
        Ok(Err(err)) => HostProbeEvent::Snapshot {
            connected: false,
            node_name: None,
            node_ip: None,
            uptime: None,
            lxc_runtime: Vec::new(),
            error: Some(err.to_string()),
        },
        Err(_) => HostProbeEvent::Snapshot {
            connected: false,
            node_name: None,
            node_ip: None,
            uptime: None,
            lxc_runtime: Vec::new(),
            error: Some("host metrics timeout".to_string()),
        },
    }
}

async fn probe_host_version() -> HostVersionEvent {
    let host_ip = std::env::var("HOST_IP").unwrap_or_else(|_| "10.10.5.250".to_string());
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    let url = format!("http://{}:8080/api/version", host_ip);
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    if !token.trim().is_empty() {
        request = request.bearer_auth(token);
    }

    let response = tokio::time::timeout(Duration::from_secs(6), request.send()).await;

    match response {
        Ok(Ok(resp)) if resp.status().is_success() => {
            match resp.json::<HostVersionResponse>().await {
                Ok(payload) => {
                    let _component = payload.component;
                    HostVersionEvent::Snapshot {
                        connected: true,
                        version: Some(payload.version),
                        latch_version: payload.latch_version,
                        error: None,
                    }
                }
                Err(err) => HostVersionEvent::Snapshot {
                    connected: false,
                    version: None,
                    latch_version: None,
                    error: Some(format!("version parse failed: {}", err)),
                },
            }
        }
        Ok(Ok(resp)) => HostVersionEvent::Snapshot {
            connected: false,
            version: None,
            latch_version: None,
            error: Some(format!("version HTTP {}", resp.status())),
        },
        Ok(Err(err)) => HostVersionEvent::Snapshot {
            connected: false,
            version: None,
            latch_version: None,
            error: Some(err.to_string()),
        },
        Err(_) => HostVersionEvent::Snapshot {
            connected: false,
            version: None,
            latch_version: None,
            error: Some("host version timeout".to_string()),
        },
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    if days > 0 {
        format!("up {}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("up {}h {}m", hours, minutes)
    } else {
        format!("up {}m", minutes)
    }
}
