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

mod app;
mod app_list;
mod backup_schedule;
mod blast_radius;
mod events;
mod gitops;
mod scaffold;
mod ssh_config;
mod stack_features;
mod telemetry;
mod theme;
mod transactions;
mod ui;

use app::App;

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
    let (sync_tx, mut sync_rx) = mpsc::unbounded_channel::<(String, bool, String)>();

    let sigint = signal::ctrl_c();
    tokio::pin!(sigint);

    // Drives mock log messages (and will drive live WebSocket ticks once connected).
    let mut log_tick = tokio::time::interval(Duration::from_millis(400));
    // Drives all visual animations: pulse, ticker, sparklines. Target ~30 FPS.
    let mut anim_tick = tokio::time::interval(Duration::from_millis(33));

    loop {
        // Drain any sync results that arrived from background tasks
        while let Ok((stack, ok, msg)) = sync_rx.try_recv() {
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

        // Promote queued batch sync work into the single-sync execution slot.
        if !app.sync_pending {
            if let Some(next_stack) = app.sync_queue.pop_front() {
                app.sync_stack = next_stack;
                app.sync_pending = true;
            }
        }

        // If a sync was queued by key actions, spawn the HTTP request now.
        if app.sync_pending {
            app.sync_pending = false;
            let stack = app.sync_stack.clone();
            let tx = sync_tx.clone();

            // Resolve LXC IP: look up `lxc-<stack>` in ~/.ssh/config, then fall
            // back to the LXC_API_IP env var, then 127.0.0.1.
            let lxc_host = format!("lxc-{}", stack);
            let ip = ssh_config::parse_ssh_config()
                .into_iter()
                .find(|e| e.host == lxc_host)
                .map(|e| e.hostname)
                .unwrap_or_else(|| {
                    std::env::var("LXC_API_IP").unwrap_or_else(|_| "127.0.0.1".to_string())
                });
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
                        let _ = tx.send((stack, true, "Sync accepted by LXC daemon".to_string()));
                    }
                    Ok(resp) => {
                        let _ = tx.send((stack, false, format!("HTTP {}", resp.status())));
                    }
                    Err(e) => {
                        let _ = tx.send((stack, false, e.to_string()));
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
