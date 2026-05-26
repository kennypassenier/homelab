//! Homelab Client TUI — entry point and event loop.
//!
//! Runs on the Linux client desktop only (never on Proxmox).
//! GitOps-first: all changes go through Git; no direct SSH deployments.

use color_eyre::eyre::Result;
use crossterm::{
    event,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::signal;

mod app;
mod app_list;
mod blast_radius;
mod events;
mod gitops;
mod scaffold;
mod ssh_config;
mod theme;
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

    let sigint = signal::ctrl_c();
    tokio::pin!(sigint);

    // Drives mock log messages (and will drive live SSE ticks once connected).
    let mut log_tick = tokio::time::interval(Duration::from_millis(400));

    loop {
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
