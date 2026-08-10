//! Homelab v2 TUI mockup — everything on screen is simulated.
//!
//! Run with: cargo run --release
//! Best experienced in a truecolor terminal at ≥ 100x30.

mod app;
mod fx;
mod sim;
mod theme;
mod ui;

use std::io::stdout;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

const ANIM_TICK: Duration = Duration::from_millis(33); // ~30 FPS
const DATA_TICK: Duration = Duration::from_millis(200);

fn main() -> std::io::Result<()> {
    // Always restore the terminal, even on panic.
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

    let mut app = App::new();
    let mut last_anim = Instant::now();
    let mut last_data = Instant::now();

    while !app.should_quit {
        // Draw at the animation cadence.
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Wait for input until the next animation frame is due.
        let budget = ANIM_TICK
            .checked_sub(last_anim.elapsed())
            .unwrap_or(Duration::ZERO);
        if event::poll(budget)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_anim.elapsed() >= ANIM_TICK {
            app.tick_anim();
            last_anim = Instant::now();
        }
        if last_data.elapsed() >= DATA_TICK {
            let dt = last_data.elapsed().as_millis() as i64;
            let logs_before = app.world.logs.len();
            app.world.tick(dt);
            // While scrolled back, anchor the view: new arrivals must not
            // shift what the user is reading.
            if !app.logs_follow {
                app.log_scroll += app.world.logs.len().saturating_sub(logs_before);
            }
            last_data = Instant::now();
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
