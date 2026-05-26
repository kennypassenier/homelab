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
    widgets::{Block, BorderType, Borders, Paragraph, Tabs, List, ListItem},
};
use std::io;
use tokio::runtime::Runtime;
use tokio::signal;
mod blast_radius;
mod gitops;
mod scaffold;
mod theme;
mod app_list;
use crate::blast_radius::{ActiveModal, draw_warning_modal, draw_delete_app_modal};
use crate::theme::Theme;
use std::fs;
use std::path::Path;
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

/// Dropdown state for a stack
struct AppDropdown {
    expanded: bool,
    selected_option: usize, // 0: Edit Config, 1: Delete App
}

struct StackDropdown {
    expanded: bool,
    selected_option: usize, // 0: add app, 1: delete stack, 2..: apps
    apps: Vec<String>,
    app_dropdowns: Vec<AppDropdown>,
}

struct App {
    active_tab: usize,
    theme: Theme,
    modal: ActiveModal,
    stacks: Vec<String>,
    selected_stack: usize,
    stack_dropdowns: Vec<StackDropdown>,
    column_focus: usize, // 0 = stacks, 1 = actions, 2 = apps
    stack_scroll: usize, // for scrolling stacks if too many
}

impl App {

    fn new() -> Self {
        let stacks = App::load_stacks();
        let stack_dropdowns = stacks.iter().map(|name| {
            let apps = crate::app_list::list_apps_for_stack(name);
            let app_dropdowns = apps.iter().map(|_| AppDropdown { expanded: false, selected_option: 0 }).collect();
            StackDropdown { expanded: false, selected_option: 0, apps, app_dropdowns }
        }).collect();
        Self {
            active_tab: 0,
            theme: Theme::cyberpunk(),
            modal: ActiveModal::None,
            stacks,
            selected_stack: 0,
            stack_dropdowns,
            column_focus: 0,
            stack_scroll: 0,
        }
    }

    fn load_stacks() -> Vec<String> {
        let mut result = Vec::new();
        if let Ok(entries) = fs::read_dir("stacks") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        result.push(name.to_string());
                    }
                }
            }
        }
        result.sort();
        result
    }

    fn reload_stacks_and_dropdowns(&mut self) {
        self.stacks = App::load_stacks();
        self.stack_dropdowns = self.stacks.iter().map(|name| {
            let apps = crate::app_list::list_apps_for_stack(name);
            let app_dropdowns = apps.iter().map(|_| AppDropdown { expanded: false, selected_option: 0 }).collect();
            StackDropdown { expanded: false, selected_option: 0, apps, app_dropdowns }
        }).collect();
        if self.selected_stack >= self.stacks.len() && !self.stacks.is_empty() {
            self.selected_stack = self.stacks.len() - 1;
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
    fn tab_right(&mut self) {
        self.next_tab();
    }
    fn tab_left(&mut self) {
        self.prev_tab();
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
                        ActiveModal::DeleteAppConfirmation { stack_name, app_name, input } => {
                            draw_delete_app_modal(f, size, stack_name, app_name, input);
                        }
                        ActiveModal::None => {
                            match app.active_tab() {
                                Tab::Scaffolding => {
                                    use ratatui::layout::{Constraint, Direction, Layout};
                                    let area = chunks[1];
                                    let col_layout = Layout::default()
                                        .direction(Direction::Horizontal)
                                        .constraints([
                                            Constraint::Length(24), // Stacks
                                            Constraint::Length(24), // Actions
                                            Constraint::Min(0),     // Apps
                                        ])
                                        .split(area);
                                    // Stacks column (scrollable)
                                    let mut stack_items = Vec::new();
                                    let visible_stacks = 20.min(app.stacks.len());
                                    let scroll = app.stack_scroll.min(app.stacks.len().saturating_sub(visible_stacks));
                                    for (i, name) in app.stacks.iter().enumerate().skip(scroll).take(visible_stacks) {
                                        let is_selected = i == app.selected_stack && app.column_focus == 0;
                                        let style = if is_selected {
                                            Style::default().fg(app.theme.accent_cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                                        } else {
                                            Style::default().fg(app.theme.text)
                                        };
                                        stack_items.push(ListItem::new(format!(" {}", name)).style(style));
                                    }
                                    let stack_block = Block::default()
                                        .borders(Borders::ALL)
                                        .border_type(BorderType::Rounded)
                                        .title("Stacks")
                                        .style(app.theme.border_style());
                                    let stack_list = List::new(stack_items)
                                        .block(stack_block)
                                        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
                                    f.render_widget(stack_list, col_layout[0]);
                                    // Actions column
                                    let mut action_items = Vec::new();
                                    let dropdown = &app.stack_dropdowns[app.selected_stack];
                                    let opts = ["add app", "delete stack"];
                                    for (j, opt) in opts.iter().enumerate() {
                                        let is_selected = app.column_focus == 1 && dropdown.selected_option == j;
                                        let mut style = Style::default();
                                        let mut prefix = "  ";
                                        if j == 0 {
                                            style = style.fg(app.theme.accent_cyan);
                                        } else if j == 1 {
                                            style = style.fg(app.theme.warning).add_modifier(Modifier::BOLD);
                                        }
                                        if is_selected {
                                            style = style.bg(app.theme.border).add_modifier(Modifier::REVERSED | Modifier::BOLD);
                                            prefix = "⮞ ";
                                        }
                                        action_items.push(ListItem::new(format!("{}{}", prefix, opt)).style(style));
                                    }
                                    let actions_block = Block::default()
                                        .borders(Borders::ALL)
                                        .border_type(BorderType::Rounded)
                                        .title("Actions")
                                        .style(app.theme.border_style());
                                    let actions_list = List::new(action_items)
                                        .block(actions_block)
                                        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
                                    f.render_widget(actions_list, col_layout[1]);
                                    // Apps column
                                    let mut app_items = Vec::new();
                                    for (k, app_name) in dropdown.apps.iter().enumerate() {
                                        let is_selected = app.column_focus == 2 && dropdown.selected_option == k + 2;
                                        let mut style = Style::default().fg(app.theme.accent_magenta);
                                        let mut prefix = "    • ";
                                        let app_dropdown = &dropdown.app_dropdowns[k];
                                        if is_selected {
                                            style = style.bg(app.theme.border).add_modifier(Modifier::BOLD | Modifier::REVERSED);
                                            prefix = "  ⮞ ";
                                        }
                                        let mut label = format!("{}{}", prefix, app_name);
                                        if app_dropdown.expanded {
                                            label.push_str("  ▼");
                                        }
                                        app_items.push(ListItem::new(label).style(style));
                                        if app_dropdown.expanded {
                                            let app_opts = ["delete app"];
                                            for (opt_idx, opt) in app_opts.iter().enumerate() {
                                                let mut style = Style::default();
                                                let mut prefix = "      ";
                                                if opt_idx == 0 {
                                                    style = style.fg(app.theme.warning).add_modifier(Modifier::BOLD);
                                                }
                                                if app_dropdown.selected_option == opt_idx {
                                                    style = style.bg(app.theme.border).add_modifier(Modifier::REVERSED | Modifier::BOLD);
                                                    prefix = "    ⮞ ";
                                                }
                                                app_items.push(ListItem::new(format!("{}{}", prefix, opt)).style(style));
                                            }
                                        }
                                    }
                                    let apps_block = Block::default()
                                        .borders(Borders::ALL)
                                        .border_type(BorderType::Rounded)
                                        .title("Apps")
                                        .style(app.theme.border_style());
                                    let apps_list = List::new(app_items)
                                        .block(apps_block)
                                        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
                                    f.render_widget(apps_list, col_layout[2]);
                                }
                                _ => {
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
                                            // Delete stack folder, commit, push
                                            let path = format!("stacks/{}", app_name);
                                            let _ = fs::remove_dir_all(&path); // ignore error for demo
                                            let _ = gitops::commit_and_push(".", &format!("Delete {}", app_name));
                                            app.modal = ActiveModal::None;
                                            // Refresh stack list after deletion
                                            app.stacks = App::load_stacks();
                                            if app.selected_stack >= app.stacks.len() && !app.stacks.is_empty() {
                                                app.selected_stack = app.stacks.len() - 1;
                                            }
                                        }
                                    } else {
                                        input.handle_event(&event::Event::Key(key));
                                    }
                                }
                                ActiveModal::DeleteAppConfirmation { stack_name, app_name, input } => {
                                    use crossterm::event::KeyCode;
                                    if key.code == KeyCode::Esc {
                                        app.modal = ActiveModal::None;
                                    } else if key.code == KeyCode::Enter {
                                        if input.value() == app_name {
                                            // Delete app folder, commit, push
                                            let path = format!("stacks/{}/{}", stack_name, app_name);
                                            let _ = fs::remove_dir_all(&path); // ignore error for demo
                                            let _ = gitops::commit_and_push(".", &format!("Delete app {} from stack {}", app_name, stack_name));
                                            app.modal = ActiveModal::None;
                                            // Refresh stack/app list after deletion
                                            app.reload_stacks_and_dropdowns();
                                        }
                                    } else {
                                        input.handle_event(&event::Event::Key(key));
                                    }
                                }
                                ActiveModal::None => {
                                    match app.active_tab() {
                                        Tab::Scaffolding => {
                                            if app.stacks.is_empty() {
                                                // nothing to do
                                            } else {
                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                match key.code {
                                                    KeyCode::Up => {
                                                        match app.column_focus {
                                                            0 => { // stacks
                                                                if app.selected_stack > 0 {
                                                                    app.selected_stack -= 1;
                                                                    if app.selected_stack < app.stack_scroll {
                                                                        app.stack_scroll = app.selected_stack;
                                                                    }
                                                                }
                                                            },
                                                            1 => { // actions
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                if dropdown.selected_option > 0 {
                                                                    dropdown.selected_option -= 1;
                                                                }
                                                            },
                                                            2 => { // apps
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                // Calculate the max selectable index, including expanded app options
                                                                let mut max_idx = 2 + dropdown.apps.len();
                                                                for (i, app_dropdown) in dropdown.app_dropdowns.iter().enumerate() {
                                                                    if app_dropdown.expanded {
                                                                        max_idx += 1; // one extra for each expanded app
                                                                    }
                                                                }
                                                                if dropdown.selected_option > 2 {
                                                                    dropdown.selected_option -= 1;
                                                                }
                                                            },
                                                            _ => {}
                                                        }
                                                    },
                                                    KeyCode::Down => {
                                                        match app.column_focus {
                                                            0 => { // stacks
                                                                if app.selected_stack + 1 < app.stacks.len() {
                                                                    app.selected_stack += 1;
                                                                    let visible_stacks = 20.min(app.stacks.len());
                                                                    if app.selected_stack >= app.stack_scroll + visible_stacks {
                                                                        app.stack_scroll += 1;
                                                                    }
                                                                }
                                                            },
                                                            1 => { // actions
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                if dropdown.selected_option + 1 < 2 {
                                                                    dropdown.selected_option += 1;
                                                                }
                                                            },
                                                            2 => { // apps
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                // Calculate the max selectable index, including expanded app options
                                                                let mut max_idx = 2 + dropdown.apps.len();
                                                                for (i, app_dropdown) in dropdown.app_dropdowns.iter().enumerate() {
                                                                    if app_dropdown.expanded {
                                                                        max_idx += 1; // one extra for each expanded app
                                                                    }
                                                                }
                                                                if dropdown.selected_option + 1 < max_idx {
                                                                    dropdown.selected_option += 1;
                                                                }
                                                            },
                                                            _ => {}
                                                        }
                                                    },
                                                    KeyCode::Left => {
                                                        if app.column_focus == 2 {
                                                            app.column_focus = 1;
                                                            let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                            dropdown.selected_option = 0;
                                                        } else if app.column_focus == 1 {
                                                            app.column_focus = 0;
                                                        }
                                                    },
                                                    KeyCode::Right => {
                                                        if app.column_focus == 0 {
                                                            app.column_focus = 1;
                                                        } else if app.column_focus == 1 {
                                                            app.column_focus = 2;
                                                            let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                            if !dropdown.apps.is_empty() {
                                                                dropdown.selected_option = 2;
                                                            }
                                                        }
                                                    },
                                                    KeyCode::Tab => {
                                                        app.tab_right();
                                                    },
                                                    KeyCode::BackTab => {
                                                        app.tab_left();
                                                    },
                                                    KeyCode::Enter | KeyCode::Char(' ') => {
                                                        // Only act in focused column
                                                        match app.column_focus {
                                                            0 => { // stacks: expand/collapse
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                dropdown.expanded = !dropdown.expanded;
                                                            },
                                                            1 => { // actions
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                match dropdown.selected_option {
                                                                    0 => {/* Add App: TODO */},
                                                                    1 => {
                                                                        let name = app.stacks[app.selected_stack].clone();
                                                                        app.modal = ActiveModal::DeleteConfirmation {
                                                                            app_name: name,
                                                                            input: Input::default(),
                                                                        };
                                                                    },
                                                                    _ => {}
                                                                }
                                                            },
                                                            2 => { // apps
                                                                let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                                let idx = dropdown.selected_option;
                                                                let stack_name = app.stacks[app.selected_stack].clone();
                                                                // Map selected_option to app or expanded option
                                                                let mut cursor = 2;
                                                                for (i, app_dropdown) in dropdown.app_dropdowns.iter_mut().enumerate() {
                                                                    if idx == cursor {
                                                                        // On app row
                                                                        if app_dropdown.expanded {
                                                                            // If already expanded, move to first expanded option (delete app)
                                                                            app.modal = ActiveModal::DeleteAppConfirmation {
                                                                                stack_name: stack_name.clone(),
                                                                                app_name: dropdown.apps[i].clone(),
                                                                                input: Input::default(),
                                                                            };
                                                                        } else {
                                                                            app_dropdown.expanded = true;
                                                                        }
                                                                        return Ok(());
                                                                    }
                                                                    cursor += 1;
                                                                    if app_dropdown.expanded {
                                                                        // On expanded option (delete app)
                                                                        if idx == cursor {
                                                                            app.modal = ActiveModal::DeleteAppConfirmation {
                                                                                stack_name: stack_name.clone(),
                                                                                app_name: dropdown.apps[i].clone(),
                                                                                input: Input::default(),
                                                                            };
                                                                            return Ok(());
                                                                        }
                                                                        cursor += 1;
                                                                    }
                                                                }
                                                            },
                                                            _ => {}
                                                        }
                                                    },
                                                    KeyCode::Esc => {
                                                        // Collapse all dropdowns
                                                        let dropdown = &mut app.stack_dropdowns[app.selected_stack];
                                                        dropdown.expanded = false;
                                                        for app_dropdown in dropdown.app_dropdowns.iter_mut() {
                                                            app_dropdown.expanded = false;
                                                        }
                                                    },
                                                    KeyCode::Char('q') => return Err(std::io::Error::new(std::io::ErrorKind::Other, "quit")),
                                                    _ => {}
                                                }
                                            }
                                        },
                                        _ => match key.code {
                                            KeyCode::Char('q') => return Err(std::io::Error::new(std::io::ErrorKind::Other, "quit")),
                                            KeyCode::Tab => app.tab_right(),
                                            KeyCode::BackTab => app.tab_left(),
                                            _ => {}
                                        }
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
