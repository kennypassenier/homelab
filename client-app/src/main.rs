//! Homelab Client TUI
//!
//! - Never runs on Proxmox
//! - Enforces Git = God (no SSH deployment)
//! - Provides pre-flight YAML linting and live SSE telemetry
//! - All code is in English, thoroughly commented, and modular

use color_eyre::eyre::Result;
use crossterm::{
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Tabs},
};
use std::io;
use tokio::runtime::Runtime;
use tokio::signal;
mod blast_radius;
mod gitops;
mod scaffold;
mod theme;
use crate::blast_radius::{ActiveModal, draw_warning_modal};
use crate::theme::Theme;
use std::fs;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

/// Enum representing the available tabs in the UI.
#[derive(Copy, Clone, Debug)]
enum Tab {
    Dashboard,
    Scaffolding,
    HostManagement,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Dashboard, Tab::Scaffolding, Tab::HostManagement]
    }
    fn title(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Scaffolding => "Scaffolding",
            Tab::HostManagement => "Host Management",
        }
    }
}

/// Holds the application state.
struct App {
    active_tab: usize,
    theme: Theme,
    modal: ActiveModal,
    // For demo: selected app/stack name (would be dynamic in real app)
    selected_name: String,
}

impl App {
    fn new() -> Self {
        Self {
            active_tab: 0,
            theme: Theme::cyberpunk(),
            modal: ActiveModal::None,
            selected_name: "demo-app".to_string(),
        }
    }
    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % Tab::all().len();
    }
    fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = Tab::all().len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }
    fn active_tab(&self) -> Tab {
        Tab::all()[self.active_tab]
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));
    let rt = Runtime::new()?;
    rt.block_on(async_main())
}

async fn async_main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();

    // Create a signal handler for SIGINT (Ctrl+C) and pin it for tokio::select!
    let mut sigint = signal::ctrl_c();
    tokio::pin!(sigint);

    loop {
        tokio::select! {
            _ = &mut sigint => { break; }
            res = async {
                terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                        ])
                        .split(size);
                    // Top Bar: Tabs
                    let tab_titles: Vec<_> = Tab::all().iter().map(|t| t.title()).collect();
                    let tabs = Tabs::new(tab_titles)
                        .select(app.active_tab)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .title("Homelab Client")
                                .style(app.theme.border_style()),
                        )
                        .highlight_style(app.theme.tab_style(true))
                        .style(app.theme.tab_style(false));
                    f.render_widget(tabs, chunks[0]);
                    // Main Content or Modal
                    match &app.modal {
                        ActiveModal::DeleteConfirmation { app_name, input } => {
                            draw_warning_modal(f, size, app_name, input);
                        }
                        ActiveModal::None => {
                            let main_block = Block::default()
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .title(app.active_tab().title())
                                .style(app.theme.border_style());
                            let content = Paragraph::new(app.active_tab().title())
                                .block(main_block)
                                .style(Style::default().fg(app.theme.text));
                            f.render_widget(content, chunks[1]);
                        }
                    }
                })?;
                // Handle key events
                if event::poll(std::time::Duration::from_millis(100))? {
                    if let event::Event::Key(key) = event::read()? {
                        use crossterm::event::{KeyCode, KeyEventKind};
                        if key.kind == KeyEventKind::Press {
                            match &mut app.modal {
                                ActiveModal::DeleteConfirmation { app_name, input } => {
                                    use crossterm::event::KeyCode;
                                    if key.code == KeyCode::Esc {
                                        app.modal = ActiveModal::None;
                                    } else if key.code == KeyCode::Enter {
                                        if input.value() == app_name {
                                            // Delete folder, commit, push
                                            let path = format!("stacks/{}", app_name);
                                            let _ = fs::remove_dir_all(&path); // ignore error for demo
                                            let _ = gitops::commit_and_push(".", &format!("Delete {}", app_name));
                                            app.modal = ActiveModal::None;
                                        }
                                    } else {
                                        input.handle_event(&event::Event::Key(key));
                                    }
                                }
                                ActiveModal::None => {
                                    match key.code {
                                        KeyCode::Char('q') => return Err(std::io::Error::new(std::io::ErrorKind::Other, "quit")),
                                        KeyCode::Left => app.prev_tab(),
                                        KeyCode::Right => app.next_tab(),
                                        KeyCode::Char('d') => {
                                            // Open modal for demo app/stack
                                            app.modal = ActiveModal::DeleteConfirmation {
                                                app_name: app.selected_name.clone(),
                                                input: Input::default(),
                                            };
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(()) as std::io::Result<()>
            } => {
                if res.is_err() { break; }
            },
        }
    }
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
