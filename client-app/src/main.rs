//! Homelab Client TUI — entry point and event loop.
//!
//! Runs on the Linux client desktop only (never on Proxmox).
//! GitOps-first: all changes go through Git; no direct SSH deployments.

use color_eyre::eyre::Result;
use crossterm::{
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::signal;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

use futures::StreamExt;
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

use app::App;
use blast_radius::{ActiveModal, OperationEntry, OperationProgressState};

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

enum RestoreDispatchEvent {
    Finished {
        stack: String,
        payload: Option<RestoreStatusPayload>,
        ok: bool,
        msg: String,
    },
}

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

/// Core event loop: draw → wait for input → handle → repeat.
async fn async_main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Channel for background tasks (sync HTTP calls) to report results back to the TUI.
    let (sync_tx, mut sync_rx) = mpsc::unbounded_channel::<SyncEvent>();
    let (restore_tx, mut restore_rx) = mpsc::unbounded_channel::<RestoreDispatchEvent>();

    let sigint = signal::ctrl_c();
    tokio::pin!(sigint);

    // Drives mock log messages (and will drive live WebSocket ticks once connected).
    let mut log_tick = tokio::time::interval(Duration::from_millis(400));
    // Drives all visual animations: pulse, ticker, sparklines. Target ~30 FPS.
    let mut anim_tick = tokio::time::interval(Duration::from_millis(33));

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
                            summary: body
                                .error_message
                                .unwrap_or_else(|| msg.clone()),
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

        // If a sync was queued by key actions, spawn the HTTP request now.
        if app.sync_pending {
            app.sync_pending = false;
            let stack = app.sync_stack.clone();
            let tx = sync_tx.clone();

            // Resolve LXC IP: look up `lxc-<stack>` in ~/.ssh/config, then fall
            // back to the LXC_API_IP env var, then 127.0.0.1.
            let ip = resolve_stack_api_ip(&stack);
            let url = format!("http://{}:8080/api/sync", ip);
            let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
            app.push_client_logfmt(
                "INFO",
                Some(&stack),
                Some("sync_dispatch"),
                &format!("POST {}", url),
                None,
            );

            tokio::spawn(async move {
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
                &format!("POST {} backup_id={}", url, backup_id),
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
                        let body = resp
                            .json::<RestoreStatusPayload>()
                            .await
                            .ok();
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

        // Always draw before polling — keeps the UI responsive
        terminal.draw(|f| ui::draw_ui(f, &app))?;

        tokio::select! {
            _ = &mut sigint => {
                disable_raw_mode()?;
                execute!(io::stdout(), LeaveAlternateScreen)?;
                return Ok(());
            }

            _ = log_tick.tick() => {
                app.tick_logs();
            }

            _ = anim_tick.tick() => {
                app.tick_anim();
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
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
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
    let mut aliases = vec![crate::scaffold::legacy_lxc_alias(stack)];

    if let Ok(config) = crate::scaffold::read_stack_config(stack) {
        aliases.push(config.hostname);
        aliases.push(crate::scaffold::canonical_lxc_name(config.vmid, stack));
    }

    let entries = ssh_config::parse_ssh_config();
    for alias in aliases {
        if let Some(entry) = entries.iter().find(|e| e.host == alias) {
            return entry.hostname.clone();
        }
    }

    std::env::var("LXC_API_IP").unwrap_or_else(|_| "127.0.0.1".to_string())
}
