//! The cyberpunk TUI (G1, AR6): Elm-style loop over a Backend.
//!
//! Some theme/fx helpers are carried over from the mockup and land in later
//! milestones (deploy focus window, sparklines); keep them available.
#![allow(dead_code)]

pub mod backend;
pub mod fx;
pub mod model;
pub mod theme;
pub mod view;

use std::io::stdout;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use backend::Backend;
use model::{update, Model, Msg};

pub async fn run(backend: Box<dyn Backend>) -> std::io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let channels = backend.start();
    let mut evt_rx = channels.evt_rx;
    let cmd_tx = channels.cmd_tx;

    let mut model = Model::new();
    // Discover locally-deployable stacks (a ./stacks dir next to the cwd).
    model.local_stacks = crate::spec::scan_local_stacks(std::path::Path::new("stacks"));
    let mut events = EventStream::new();
    let mut anim = tokio::time::interval(Duration::from_millis(33));
    anim.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !model.should_quit {
        terminal.draw(|f| view::draw(f, &model))?;

        tokio::select! {
            maybe = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe {
                    if key.kind == KeyEventKind::Press {
                        update(&mut model, Msg::Key(key));
                    }
                }
            }
            Some(bev) = evt_rx.recv() => {
                update(&mut model, Msg::Backend(bev));
            }
            _ = anim.tick() => {
                update(&mut model, Msg::Tick);
            }
        }

        // Flush queued commands from the pure update to the backend.
        for cmd in model.outbox.drain(..) {
            let _ = cmd_tx.send(cmd).await;
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
