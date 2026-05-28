use crossterm::{
    ExecutableCommand,
    event::{Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    prelude::*,
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};
use std::path::Path;

mod app;
mod backup;
mod hardware;
mod policy;
mod self_update;
mod storage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<_>> = Terminal::new(backend)?;

    let mut app = app::App::new();
    let mut last_time = std::time::Instant::now();

    // Channel for the backup thread to send status lines back to the TUI
    let (backup_tx, backup_rx) = std::sync::mpsc::channel::<String>();
    backup::start_policy_enforcer(backup_tx.clone());

    loop {
        // Poll backup status from background thread
        while let Ok(line) = backup_rx.try_recv() {
            app.backup_status.push(line);
            if app.backup_status.len() > 200 {
                app.backup_status.remove(0);
            }
            if app
                .backup_status
                .last()
                .map(|l| l.starts_with("DONE"))
                .unwrap_or(false)
            {
                app.backup_running = false;
            }
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Down | KeyCode::Char('j' | 'n') => {
                            app.tab = (app.tab + 1).min(4);
                        }
                        KeyCode::Up | KeyCode::Char('k' | 'p') => {
                            if app.tab > 0 {
                                app.tab -= 1;
                            }
                        }
                        KeyCode::Char('1') => app.tab = 0,
                        KeyCode::Char('2') => app.tab = 1,
                        KeyCode::Char('3') => app.tab = 2,
                        KeyCode::Char('4') => app.tab = 3,
                        KeyCode::Char('5') => app.tab = 4,
                        // 'b' triggers the backup orchestration cycle
                        KeyCode::Char('b') if !app.backup_running => {
                            app.backup_running = true;
                            app.backup_status.push("Starting backup cycle…".to_string());
                            let stacks: Vec<(String, String)> = app
                                .lxc_nodes()
                                .into_iter()
                                .filter(|n| n.status == "RUN")
                                .map(|n| {
                                    let stack =
                                        n.name.strip_prefix("lxc-").unwrap_or(&n.name).to_string();
                                    (stack, n.ip)
                                })
                                .collect();
                            let tx = backup_tx.clone();
                            std::thread::spawn(move || {
                                for (stack, ip) in &stacks {
                                    let _ = tx.send(format!(
                                        "[{}] Pausing containers via {}:8080…",
                                        stack, ip
                                    ));
                                }
                                match backup::run_backup_cycle_owned_guarded(stacks) {
                                    Some(results) => {
                                        for r in &results {
                                            let status = if r.backup_ok { "OK" } else { "FAIL" };
                                            let _ = tx.send(format!(
                                                "[{}] pause={} backup={} resume={} — {}",
                                                r.stack,
                                                if r.paused { "ok" } else { "err" },
                                                status,
                                                if r.resumed { "ok" } else { "err" },
                                                r.message
                                            ));
                                        }
                                    }
                                    None => {
                                        let _ = tx.send(
                                            "Backup skipped: another cycle is currently running"
                                                .to_string(),
                                        );
                                    }
                                }
                                let _ = tx.send("DONE".to_string());
                            });
                        }
                        KeyCode::Char('U') => {
                            app.backup_status.push("Checking HOST updates…".to_string());
                            match self_update::check_and_apply_update() {
                                Ok(msg) => app.backup_status.push(msg),
                                Err(err) => app
                                    .backup_status
                                    .push(format!("HOST update failed: {}", err)),
                            }
                        }
                        KeyCode::Char('o') => {
                            let gitops_root = std::env::var("GITOPS_REPO")
                                .unwrap_or_else(|_| "/opt/gitops".to_string());
                            for line in
                                policy::reconcile_boot_policies(Path::new(&gitops_root), false)
                            {
                                app.backup_status.push(line);
                            }
                        }
                        KeyCode::Char('O') => {
                            let gitops_root = std::env::var("GITOPS_REPO")
                                .unwrap_or_else(|_| "/opt/gitops".to_string());
                            for line in
                                policy::reconcile_boot_policies(Path::new(&gitops_root), true)
                            {
                                app.backup_status.push(line);
                            }
                        }
                        KeyCode::Char('h') => {
                            let gitops_root = std::env::var("GITOPS_REPO")
                                .unwrap_or_else(|_| "/opt/gitops".to_string());
                            for line in
                                policy::reconcile_hot_resources(Path::new(&gitops_root), false)
                            {
                                app.backup_status.push(line);
                            }
                        }
                        KeyCode::Char('H') => {
                            let gitops_root = std::env::var("GITOPS_REPO")
                                .unwrap_or_else(|_| "/opt/gitops".to_string());
                            for line in
                                policy::reconcile_hot_resources(Path::new(&gitops_root), true)
                            {
                                app.backup_status.push(line);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_time.elapsed() > std::time::Duration::from_millis(100) {
            terminal.draw(|f| draw_main(f, &app))?;
            last_time = std::time::Instant::now();
        }
    }

    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

fn draw_main(f: &mut ratatui::Frame, app: &app::App) {
    let screen = f.area();
    let area = Rect::new(0, 1, screen.width, screen.height.saturating_sub(1));

    match app.tab {
        0 => draw_dashboard(f, app, area),
        1 => draw_lxc_nodes(f, app, area),
        2 => draw_backups(f, app, area),
        3 => draw_storage(f, app, area),
        4 => draw_hardware(f, app, area),
        _ => unreachable!(),
    }

    let footer = Paragraph::new(
        " HOST v0.1 | TABS: ↑↓/1-5 | [b] backup | [o/O] boot preview/apply | [h/H] resources preview/apply | [U] self-update | q=quit",
    )
    .style(Style::default().fg(Color::DarkGray));
    let fx = area.x + area.width - 2;
    let fy = area.y + area.height - 1;
    let footer_area = Rect::new(fx, fy, 2, 1);
    f.render_widget(footer, footer_area);
}

fn draw_dashboard(f: &mut ratatui::Frame, app: &app::App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(" >> HOST_MESH << ").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        chunks[0],
    );

    let spark = " CPU: ▃▄▅▃▄▅▄▃ 14%   RAM: ▄▅▄▅▄▅▄▅ 6.2/32 GB   DISK: ██████░░ 214/512 GB (42%) ";
    f.render_widget(Paragraph::new(spark), chunks[1]);

    let sub_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(22)])
        .split(chunks[2]);

    draw_lxc_mesh_table(f, app, sub_chunks[0]);
    draw_backup_status(f, app.backup_stack(), sub_chunks[1]);
}

fn draw_lxc_nodes(f: &mut ratatui::Frame, app: &app::App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    f.render_widget(
        Paragraph::new(" >> LXC_CORE << ").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        chunks[0],
    );

    let sub_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(32)])
        .split(chunks[1]);

    draw_lxc_mesh_table(f, app, sub_chunks[0]);
    draw_detail_view(f, sub_chunks[1]);
}

fn draw_backups(f: &mut ratatui::Frame, app: &app::App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let running = app.backup_running;
    let header_text = if running {
        " >> BACKUP_ORCHESTRATOR << [RUNNING…] "
    } else {
        " >> BACKUP_ORCHESTRATOR << "
    };
    f.render_widget(
        Paragraph::new(header_text).style(
            Style::default()
                .fg(if running { Color::Yellow } else { Color::Cyan })
                .add_modifier(Modifier::BOLD),
        ),
        chunks[0],
    );

    let hint = if running {
        "Backup in progress — wait for DONE…"
    } else {
        "[b] Start backup cycle   [q] Quit"
    };
    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            ),
        chunks[1],
    );

    let log_lines: Vec<ratatui::text::Line> = if app.backup_status.is_empty() {
        vec![ratatui::text::Line::from(
            "No backup runs yet. Press [b] on this tab to start.",
        )]
    } else {
        app.backup_status
            .iter()
            .rev()
            .take(area.height as usize)
            .map(|l| {
                let colour = if l.contains("FAIL") || l.contains("err") {
                    Color::Red
                } else if l.starts_with("DONE") || l.contains("OK") {
                    Color::Green
                } else {
                    Color::White
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    l.as_str(),
                    Style::default().fg(colour),
                ))
            })
            .collect()
    };

    f.render_widget(
        Paragraph::new(log_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Backup log (newest first) "),
        ),
        chunks[2],
    );
}

fn draw_storage(f: &mut ratatui::Frame, _app: &app::App, area: Rect) {
    let appdata_root = std::env::var("APPDATA_BASE").unwrap_or_else(|_| "/opt/appdata".to_string());
    let mut lines = vec![
        format!("Storage root: {}", appdata_root),
        "Bind mount preflight + stack storage health:".to_string(),
    ];

    match storage::get_storage_summary(Path::new(&appdata_root)) {
        Ok(statuses) => {
            let healthy = statuses
                .iter()
                .filter(|s| matches!(s.health, storage::StorageHealth::Healthy))
                .count();
            let warning = statuses
                .iter()
                .filter(|s| matches!(s.health, storage::StorageHealth::Warning))
                .count();
            let critical = statuses
                .iter()
                .filter(|s| matches!(s.health, storage::StorageHealth::Critical))
                .count();

            lines.push(format!(
                "Stacks inspected: {} (healthy={}, warning={}, critical={})",
                statuses.len(),
                healthy,
                warning,
                critical
            ));

            for status in statuses.iter().take(10) {
                lines.push(format!(
                    "- {:<14} {} ({})",
                    status.stack_name, status.health, status.message
                ));
            }

            if statuses.len() > 10 {
                lines.push(format!("... and {} more stack(s)", statuses.len() - 10));
            }
        }
        Err(e) => {
            lines.push(format!("Storage inspection failed: {}", e));
        }
    }

    let content = lines.join("\n");

    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" STORAGE "),
        ),
        area,
    );
}

fn draw_hardware(f: &mut ratatui::Frame, _app: &app::App, area: Rect) {
    let gitops_root = std::env::var("GITOPS_REPO").unwrap_or_else(|_| "/opt/gitops".to_string());
    let gpu = hardware::check_gpu_readiness();
    let tun = hardware::check_tun_readiness();

    let mut lines = vec![
        format!("GPU readiness: {} ({})", gpu.readiness, gpu.message),
        format!("TUN readiness: {} ({})", tun.readiness, tun.message),
        "Per-stack intent reconciliation:".to_string(),
    ];

    match hardware::discover_stack_hardware_intents(Path::new(&gitops_root)) {
        Ok(intents) if intents.is_empty() => {
            lines.push("No hardware intents found in lxc-compose.yml files".to_string());
        }
        Ok(intents) => {
            for intent in intents.iter().take(12) {
                let result = hardware::reconcile_hardware(intent);
                let mark = if result.success { "OK" } else { "FAIL" };
                lines.push(format!(
                    "- {:<14} {:<3} {}",
                    result.stack_name, mark, result.message
                ));
            }

            if intents.len() > 12 {
                lines.push(format!("... and {} more intent(s)", intents.len() - 12));
            }
        }
        Err(e) => {
            lines.push(format!("Intent discovery failed: {}", e));
        }
    }

    let content = lines.join("\n");

    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" HARDWARE "),
        ),
        area,
    );
}

fn draw_lxc_mesh_table(f: &mut ratatui::Frame, app: &app::App, area: Rect) {
    let nodes = app.lxc_nodes();
    let header_style = Style::default().add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from(" STATUS ").style(header_style),
        Cell::from(" ID   CONTAINER ").style(header_style),
        Cell::from(" CPU    RAM      ").style(header_style),
    ]);

    let rows: Vec<Row> = nodes
        .iter()
        .map(|n| {
            let style = if n.status == "RUN" && n.cpu > 0.0 {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let status_str = format!(
                "{} {}",
                if n.status == "RUN" {
                    "● RUN"
                } else {
                    "○ STP"
                },
                n.status
            );

            Row::new(vec![
                Cell::from(status_str).style(style),
                Cell::from(format!(" {} {}", n.id, n.name)).style(Style::default()),
                Cell::from(format!(
                    "{:>3}%  {}",
                    n.cpu as u64,
                    n.ram.as_deref().unwrap_or("—")
                ))
                .style(Style::default()),
            ])
        })
        .collect();

    let title = format!(" LXC_MESH :: {} NODES ", nodes.len());

    f.render_widget(
        Table::new(
            rows,
            vec![
                Constraint::Length(7),
                Constraint::Min(16),
                Constraint::Length(14),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .title(title.as_str())
                .style(Style::default()),
        ),
        area,
    );
}

fn draw_backup_status(f: &mut ratatui::Frame, bs: app::BackupStack, area: Rect) {
    let hints = vec![("u", "update now"), ("b", "run backup now")];
    let mut content = format!(
        "Current:  {}\nLatest:   {}  ● AVAILABLE\nChannel:  github releases\nStatus:   [IDLE]",
        bs.repo, "v1.5.0"
    );
    let hint_lines = hints
        .iter()
        .map(|(k, v)| format!("\n  [{}] {}", k, v))
        .collect::<String>();
    content.push_str(&hint_lines);

    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" BACKUP_STATUS :: Restic "),
        ),
        area,
    );
}

fn draw_detail_view(f: &mut ratatui::Frame, area: Rect) {
    let content = r#"lxc-gateway                    
─────────────────────          
VMID:   103                     
Stack:  gateway                
Disk:   4.2/8 GB               
Cores:  2                      
RAM:    1024 MB                 
GPU:    ✗                      
TUN:    ✓                       
State:  ● RUNNING             
                                 
[ ] passthrough                "#;

    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" DETAIL "),
        ),
        area,
    );
}
