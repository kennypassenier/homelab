use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs},
    Terminal,
};
use std::sync::{Arc, Mutex};

mod api;
mod app;
mod docker;
mod gitops;
mod mounts;
mod restore;

use app::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(Mutex::new(AppState::new()));

    // Spawn background tasks — these run concurrently while the TUI blocks the main thread
    tokio::spawn(api::run_server(state.clone()));
    tokio::spawn(docker::run_poller(state.clone()));
    tokio::spawn(gitops::run_checker(state.clone()));
    tokio::spawn(mounts::run_checker(state.clone()));

    // block_in_place allows running blocking code (TUI) inside a tokio runtime
    tokio::task::block_in_place(|| run_tui(state))?;

    Ok(())
}

fn run_tui(state: Arc<Mutex<AppState>>) -> Result<(), Box<dyn std::error::Error>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        {
            let s = state.lock().unwrap();
            terminal.draw(|f| draw(f, &s))?;
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut s = state.lock().unwrap();
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Tab | KeyCode::Right => s.next_tab(),
                        KeyCode::BackTab | KeyCode::Left => s.prev_tab(),
                        KeyCode::Char('1') => s.tab = 0,
                        KeyCode::Char('2') => s.tab = 1,
                        KeyCode::Char('3') => s.tab = 2,
                        KeyCode::Char('4') => s.tab = 3,
                        KeyCode::Char('5') => s.tab = 4,
                        KeyCode::Char('s') => {
                            s.sync_requested = true;
                            s.add_log(app::LogLevel::Info, "Manual sync triggered".to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, state: &AppState) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // --- Tab bar ---
    let tab_titles: Vec<Line> = ["Dashboard", "GitOps", "Containers", "Secrets", "Logs"]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i == state.tab {
                Line::styled(
                    *t,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Line::styled(*t, Style::default().fg(Color::White))
            }
        })
        .collect();

    let mut title = format!(" LXC_DAEMON :: {} ", state.stack_name.to_uppercase());
    if state.is_syncing {
        title.push_str(" \u{27f3} SYNCING");
    }
    if state.backup_paused {
        title.push_str(" \u{23f8} BACKUP PAUSED");
    }

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title),
        )
        .select(state.tab)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // --- Content ---
    match state.tab {
        0 => draw_dashboard(f, state, chunks[1]),
        1 => draw_gitops(f, state, chunks[1]),
        2 => draw_containers(f, state, chunks[1]),
        3 => draw_secrets(f, state, chunks[1]),
        4 => draw_logs(f, state, chunks[1]),
        _ => {}
    }

    // --- Footer / status ticker ---
    let running = state
        .containers
        .iter()
        .filter(|c| c.status.contains("UP"))
        .count();
    let total = state.containers.len();
    let gitops_label = if state.git.is_synced {
        "SYNCED"
    } else {
        "DRIFTED"
    };
    let mount_label = if state.mounts.docker_ok && state.mounts.config_ok {
        "MOUNTS OK"
    } else {
        "MOUNT FAIL"
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            "\u{25cf} ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{} [ONLINE] :: {}/{} UP :: GITOPS {} :: {} :: API :8080",
                state.stack_name.to_uppercase(),
                running,
                total,
                gitops_label,
                mount_label,
            ),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    f.render_widget(footer, chunks[2]);
}

// \u2500\u2500\u2500 Dashboard \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

fn draw_dashboard(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let running = state
        .containers
        .iter()
        .filter(|c| c.status.contains("UP"))
        .count();
    let total = state.containers.len();
    let gitops_color = if state.git.is_synced {
        Color::Green
    } else {
        Color::Yellow
    };
    let gitops_text = if state.git.is_synced {
        "\u{2713} SYNCED"
    } else {
        "\u{26a0} DRIFTED"
    };
    let secrets_text = if state.secrets.loaded {
        "LOADED"
    } else {
        "NOT LOADED"
    };

    let banner = Paragraph::new(vec![
        Line::styled(
            format!(
                "  >> LXC_CORE <<   Stack: {}  \u{b7}  IP: {}  \u{b7}  Uptime: {}",
                state.stack_name, state.stack_ip, state.uptime
            ),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled("  GitOps: ", Style::default().fg(Color::White)),
            Span::styled(gitops_text, Style::default().fg(gitops_color)),
            Span::styled(
                format!(
                    "  \u{b7}  Last sync: {}  \u{b7}  Containers: {}/{} UP  \u{b7}  Secrets: {}",
                    state.git.last_sync, running, total, secrets_text
                ),
                Style::default().fg(Color::White),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" STACK_STATUS :: {} ", state.stack_name)),
    );
    f.render_widget(banner, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(36)])
        .split(chunks[1]);

    draw_containers_table(f, state, main_chunks[0]);
    draw_gitops_sidebar(f, state, main_chunks[1]);

    let hints = Paragraph::new(Line::styled(
        "  [Tab/1-5] nav   [s] sync now   [q] quit",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(hints, chunks[2]);
}

fn draw_containers_table(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let header = Row::new([
        Cell::from("STATUS").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("NAME").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("IMAGE").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("UPTIME").style(Style::default().add_modifier(Modifier::BOLD)),
    ]);

    let rows: Vec<Row> = if state.containers.is_empty() {
        vec![Row::new([
            Cell::from("Connecting to Docker...").style(Style::default().fg(Color::DarkGray)),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        state
            .containers
            .iter()
            .map(|c| {
                let color = if c.status.contains("UP") {
                    Color::Green
                } else {
                    Color::Red
                };
                Row::new([
                    Cell::from(c.status.clone()).style(Style::default().fg(color)),
                    Cell::from(c.name.clone()),
                    Cell::from(c.image.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(c.uptime.clone()),
                ])
            })
            .collect()
    };

    let running = state
        .containers
        .iter()
        .filter(|c| c.status.contains("UP"))
        .count();
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(15),
            Constraint::Min(0),
            Constraint::Length(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(format!(" CONTAINERS :: {} RUNNING ", running))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(table, area);
}

fn draw_gitops_sidebar(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let synced_color = if state.git.is_synced {
        Color::Green
    } else {
        Color::Yellow
    };
    let synced_text = if state.git.is_synced {
        "\u{2713} UP TO DATE"
    } else {
        "\u{26a0} BEHIND"
    };
    let lock_color = if state.git.lock_free {
        Color::Green
    } else {
        Color::Yellow
    };
    let lock_text = if state.git.lock_free {
        "FREE"
    } else {
        "\u{26a0} LOCKED"
    };

    let content = vec![
        Line::styled(
            format!("Branch:  {}", state.git.branch),
            Style::default().fg(Color::White),
        ),
        Line::styled(
            format!("Commit:  {}", state.git.commit),
            Style::default().fg(Color::Green),
        ),
        Line::styled(
            format!("Sparse:  {}", state.git.sparse),
            Style::default().fg(Color::White),
        ),
        Line::from(vec![
            Span::styled("Status:  ", Style::default().fg(Color::White)),
            Span::styled(synced_text, Style::default().fg(synced_color)),
        ]),
        Line::from(vec![
            Span::styled("Lock:    ", Style::default().fg(Color::White)),
            Span::styled(lock_text, Style::default().fg(lock_color)),
        ]),
        Line::raw(""),
        Line::styled("API: \u{25cf} :8080", Style::default().fg(Color::Green)),
        Line::styled("Cron: every 30 min", Style::default().fg(Color::White)),
        Line::raw(""),
        Line::styled("[s] sync now", Style::default().fg(Color::DarkGray)),
    ];

    let para = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" GITOPS_ENGINE ")
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(para, area);
}

// \u2500\u2500\u2500 GitOps tab \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

fn draw_gitops(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(40)])
        .split(area);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(chunks[0]);

    let synced_color = if state.git.is_synced {
        Color::Green
    } else {
        Color::Yellow
    };
    let synced_text = if state.git.is_synced {
        "\u{2713} SYNCED [CLEAN]"
    } else {
        "\u{26a0} BEHIND REMOTE"
    };
    let lock_color = if state.git.lock_free {
        Color::Green
    } else {
        Color::Yellow
    };
    let lock_text = if state.git.lock_free {
        "\u{25cf} FREE"
    } else {
        "\u{26a0} LOCKED"
    };

    let gitops_content = vec![
        Line::styled(
            format!("Repo:     {}", state.git.repo_url),
            Style::default().fg(Color::White),
        ),
        Line::styled(
            format!(
                "Branch:   {}  \u{b7}  Commit: {}",
                state.git.branch, state.git.commit
            ),
            Style::default().fg(Color::White),
        ),
        Line::styled(
            format!("Sparse:   {}", state.git.sparse),
            Style::default().fg(Color::White),
        ),
        Line::from(vec![
            Span::styled(
                "Lock:     /tmp/gitops.lock  ",
                Style::default().fg(Color::White),
            ),
            Span::styled(lock_text, Style::default().fg(lock_color)),
        ]),
        Line::raw(""),
        Line::styled(
            format!("Last sync:   {}", state.git.last_sync),
            Style::default().fg(Color::White),
        ),
        Line::styled(
            format!("Next cron:   {}", state.git.next_sync),
            Style::default().fg(Color::White),
        ),
        Line::from(vec![
            Span::styled("Status:      ", Style::default().fg(Color::White)),
            Span::styled(synced_text, Style::default().fg(synced_color)),
        ]),
        Line::raw(""),
        Line::styled(
            "HTTP Push:   \u{25cf} LISTENING  0.0.0.0:8080",
            Style::default().fg(Color::Green),
        ),
        Line::styled(
            "             POST /api/sync",
            Style::default().fg(Color::DarkGray),
        ),
        Line::styled(
            "             POST /api/backup/pause",
            Style::default().fg(Color::DarkGray),
        ),
        Line::styled(
            "             POST /api/backup/resume",
            Style::default().fg(Color::DarkGray),
        ),
    ];

    let gitops_panel = Paragraph::new(gitops_content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(format!(" GITOPS_ENGINE :: {} ", state.stack_name))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(gitops_panel, left_chunks[0]);

    let hints = Paragraph::new(Line::styled(
        "  [s] sync now   [f] force redeploy   [g] GC orphans",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(hints, left_chunks[1]);

    // Right sidebar
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Min(0)])
        .split(chunks[1]);

    let sync_log_items: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(5)
        .map(|entry| {
            let color = match entry.level {
                app::LogLevel::Error => Color::Red,
                app::LogLevel::Warn => Color::Yellow,
                app::LogLevel::Ok | app::LogLevel::Info => Color::Green,
                app::LogLevel::Debug => Color::DarkGray,
            };
            ListItem::new(Line::styled(
                format!("{}  {}", entry.timestamp.format("%H:%M"), entry.msg),
                Style::default().fg(color),
            ))
        })
        .collect();

    let sync_log = List::new(sync_log_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" SYNC_LOG (last 5) ")
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(sync_log, right_chunks[0]);

    let rollback = Paragraph::new(vec![
        Line::styled(
            "Auto-rollback: \u{25cf} ARMED (10s)",
            Style::default().fg(Color::Green),
        ),
        Line::styled(
            "Watches containers after deploy",
            Style::default().fg(Color::DarkGray),
        ),
        Line::raw(""),
        Line::styled(
            "Known-good IDs stored in state",
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" ROLLBACK_GUARD ")
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(rollback, right_chunks[1]);
}

// \u2500\u2500\u2500 Containers tab \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

fn draw_containers(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let header = Row::new([
        Cell::from("STATUS").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("NAME").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("IMAGE").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("PORTS").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("UPTIME").style(Style::default().add_modifier(Modifier::BOLD)),
    ]);

    let rows: Vec<Row> = if state.containers.is_empty() {
        vec![Row::new([
            Cell::from("No containers \u{2014} is Docker running?")
                .style(Style::default().fg(Color::DarkGray)),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        state
            .containers
            .iter()
            .map(|c| {
                let color = if c.status.contains("UP") {
                    Color::Green
                } else {
                    Color::Red
                };
                Row::new([
                    Cell::from(c.status.clone()).style(Style::default().fg(color)),
                    Cell::from(c.name.clone()),
                    Cell::from(c.image.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(c.ports.clone()),
                    Cell::from(c.uptime.clone()),
                ])
            })
            .collect()
    };

    let running = state
        .containers
        .iter()
        .filter(|c| c.status.contains("UP"))
        .count();
    let total = state.containers.len();

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(15),
            Constraint::Min(0),
            Constraint::Length(15),
            Constraint::Length(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(format!(
                " CONTAINER_MESH :: {} :: {}/{} UP ",
                state.stack_name, running, total
            ))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(table, chunks[0]);

    let hints = Paragraph::new(Line::styled(
        "  \u{2191}/\u{2193} select   [r] restart   [x] stop   [l] view logs   [e] exec shell",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(hints, chunks[1]);
}

// \u2500\u2500\u2500 Secrets tab \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

fn draw_secrets(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let status_color = if state.secrets.loaded {
        Color::Green
    } else {
        Color::Red
    };
    let status_text = if state.secrets.loaded {
        format!(
            "\u{2713} SECRETS LOADED  \u{b7}  Loaded: {}",
            state.secrets.loaded_ago
        )
    } else {
        "\u{2717} SECRETS NOT LOADED".to_string()
    };

    let docker_color = if state.mounts.docker_ok {
        Color::Green
    } else {
        Color::Red
    };
    let config_color = if state.mounts.config_ok {
        Color::Green
    } else {
        Color::Red
    };
    let docker_mount_text = if state.mounts.docker_ok {
        format!("\u{2713} MOUNTED  (st_dev {})", state.mounts.docker_dev)
    } else {
        "\u{2717} NOT MOUNTED \u{2014} bind mount missing".to_string()
    };
    let config_mount_text = if state.mounts.config_ok {
        format!("\u{2713} MOUNTED  (st_dev {})", state.mounts.config_dev)
    } else {
        "\u{2717} NOT MOUNTED \u{2014} bind mount missing".to_string()
    };

    let mut content = vec![
        Line::styled(
            format!("Method:    {}", state.secrets.method),
            Style::default().fg(Color::White),
        ),
        Line::styled(
            format!("Target:    {}", state.secrets.target),
            Style::default().fg(Color::White),
        ),
        Line::from(vec![
            Span::styled("Status:    ", Style::default().fg(Color::White)),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]),
        Line::raw(""),
        Line::styled(
            " Last run:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if state.secrets.last_run_log.is_empty() {
        content.push(Line::styled(
            " No run recorded yet.",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for (line, ok) in &state.secrets.last_run_log {
            let color = if *ok { Color::Green } else { Color::DarkGray };
            content.push(Line::styled(
                format!(" {}", line),
                Style::default().fg(color),
            ));
        }
    }

    content.push(Line::raw(""));
    content.push(Line::styled(
        " Mount check:",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));
    content.push(Line::from(vec![
        Span::styled(
            format!("  {}   ", state.mounts.docker_path),
            Style::default().fg(Color::White),
        ),
        Span::styled(docker_mount_text, Style::default().fg(docker_color)),
    ]));
    content.push(Line::from(vec![
        Span::styled(
            format!("  {}   ", state.mounts.config_path),
            Style::default().fg(Color::White),
        ),
        Span::styled(config_mount_text, Style::default().fg(config_color)),
    ]));

    let panel = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(" SECRETS_ENGINE :: Ephemeral Container ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(panel, chunks[0]);

    let hints = Paragraph::new(Line::styled(
        "  [r] reload secrets   [v] view .env keys (redacted)",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(hints, chunks[1]);
}

// \u2500\u2500\u2500 Logs tab \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

fn draw_logs(f: &mut ratatui::Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let log_items: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(200)
        .map(|entry| {
            let (color, label) = match entry.level {
                app::LogLevel::Error => (Color::Red, "ERROR"),
                app::LogLevel::Warn => (Color::Yellow, "WARN "),
                app::LogLevel::Ok => (Color::Green, "OK   "),
                app::LogLevel::Info => (Color::Cyan, "INFO "),
                app::LogLevel::Debug => (Color::DarkGray, "DEBUG"),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", entry.timestamp.format("%H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("[{}] ", label), Style::default().fg(color)),
                Span::styled(entry.msg.clone(), Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let list = List::new(log_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(format!(" LOGS ({} entries) ", state.logs.len()))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(list, chunks[0]);

    let hints = Paragraph::new(Line::styled(
        "  [u] scroll up    [d] scroll down    [c] clear",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(hints, chunks[1]);
}
