//! All terminal rendering logic for the homelab client TUI.
//!
//! The single public entry point is `draw_ui`, called unconditionally every
//! iteration of the event loop before polling for input.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs},
};

use crate::app::{App, LOG_SOURCES, LogLevelFilter, Tab};
use crate::blast_radius::{
    ActiveModal, draw_app_config_editor, draw_app_creation_wizard, draw_delete_app_modal,
    draw_operation_progress, draw_ssh_add_wizard, draw_stack_config_editor,
    draw_stack_creation_wizard, draw_warning_modal,
};

/// Renders the complete UI for the current frame.
pub fn draw_ui(f: &mut Frame, app: &App) {
    let size = f.size();

    // ── Minimum terminal size guard ──────────────────────────────────────────
    if size.width < 80 || size.height < 24 {
        f.render_widget(
            Paragraph::new(format!(
                "\u{26a0}  TERMINAL TOO SMALL \u{2014} minimum 80\u{d7}24  (current {}x{})",
                size.width, size.height
            ))
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            size,
        );
        return;
    }

    // Root layout: tab bar (3) | body (fill) | ticker (1)
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    // ── Tab bar ─────────────────────────────────────────────────────────────
    let tab_titles: Vec<_> = Tab::all().iter().map(|t| t.title()).collect();
    let tabs = Tabs::new(tab_titles)
        .select(app.active_tab)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ SYS_CORE :: CLIENT ] ")
                .style(app.theme.active_border_style()),
        )
        .highlight_style(app.theme.tab_style(true))
        .style(app.theme.tab_style(false));
    f.render_widget(tabs, root[0]);

    // ── Body: modals take priority over tab content ──────────────────────────
    match &app.modal {
        ActiveModal::DeleteConfirmation { app_name, input } => {
            draw_warning_modal(f, size, app_name, input);
        }
        ActiveModal::DeleteAppConfirmation {
            stack_name,
            app_name,
            input,
        } => {
            draw_delete_app_modal(f, size, stack_name, app_name, input);
        }
        ActiveModal::AppCreationWizard(state) => {
            draw_app_creation_wizard(f, size, state);
        }
        ActiveModal::AppConfigEditor(state) => {
            draw_app_config_editor(f, size, state);
        }
        ActiveModal::StackCreationWizard(state) => {
            draw_stack_creation_wizard(f, size, state);
        }
        ActiveModal::StackConfigEditor(state) => {
            draw_stack_config_editor(f, size, state);
        }
        ActiveModal::OperationProgress(state) => {
            draw_operation_progress(f, size, state);
        }
        ActiveModal::SshAddWizard(state) => {
            draw_ssh_add_wizard(f, size, state);
        }
        ActiveModal::None => match app.active_tab() {
            Tab::Scaffolding => draw_scaffolding(f, root[1], app),
            Tab::Dashboard => draw_dashboard(f, root[1], app),
            Tab::Backups => draw_backups(f, root[1], app),
            Tab::HostManagement => draw_host_management(f, root[1], app),
            Tab::Logs => draw_logs(f, root[1], app),
        },
    }

    // ── Telemetry ticker (bottom row, always visible) ────────────────────────
    draw_ticker_bar(f, root[2], app);
}

// ── Tab renderers ────────────────────────────────────────────────────────────

fn draw_scaffolding(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24), // stacks
            Constraint::Length(24), // actions
            Constraint::Min(0),     // apps
        ])
        .split(rows[0]);

    // ── Stacks column ──────────────────────────────────────────────────────
    let visible = app.stacks.len().min(20);
    let scroll = app
        .stack_scroll
        .min(app.stacks.len().saturating_sub(visible));
    let stack_items: Vec<ListItem> = app
        .stacks
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, name)| {
            let selected = i == app.selected_stack && app.column_focus == 0;
            let style = if selected {
                crate::theme::Theme::pulse_style(app.pulse_phase).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.text)
            };
            ListItem::new(format!(" {}", name)).style(style)
        })
        .collect();

    f.render_widget(
        List::new(stack_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ STACKS ] ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(if app.column_focus == 0 {
                    app.theme.active_border_style()
                } else {
                    app.theme.border_style()
                }),
        ),
        cols[0],
    );

    // Nothing else to render if there are no stacks yet
    if app.stacks.is_empty() {
        f.render_widget(
            Paragraph::new("  no stacks yet  |  press [n] to create stack")
                .style(Style::default().fg(Color::DarkGray)),
            rows[1],
        );
        return;
    }

    // ── Actions column ─────────────────────────────────────────────────────
    let actions = ["+ add app", "✗ delete stack", "≡ stack config"];
    let dropdown = &app.stack_dropdowns[app.selected_stack];
    let action_items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(j, label)| {
            let selected = app.column_focus == 1 && dropdown.selected_option == j;
            let base = match j {
                1 => Style::default()
                    .fg(app.theme.warning)
                    .add_modifier(Modifier::BOLD),
                _ => Style::default().fg(app.theme.accent_cyan),
            };
            let style = if selected {
                base.add_modifier(Modifier::BOLD).bg(Color::Rgb(0, 80, 90))
            } else {
                base
            };
            ListItem::new(format!(" {}", label)).style(style)
        })
        .collect();

    f.render_widget(
        List::new(action_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ ACTIONS ] ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(if app.column_focus == 1 {
                    app.theme.active_border_style()
                } else {
                    app.theme.border_style()
                }),
        ),
        cols[1],
    );

    // ── Apps column ────────────────────────────────────────────────────────
    let dropdown = &app.stack_dropdowns[app.selected_stack];
    let mut items: Vec<ListItem> = Vec::new();
    let mut render_idx: usize = 2; // first two logical indices are reserved for actions

    for (i, app_name) in dropdown.apps.iter().enumerate() {
        let selected = app.column_focus == 2 && dropdown.selected_option == render_idx;
        let prefix = if dropdown.app_dropdowns[i].expanded {
            "\u{25bc} "
        } else {
            "\u{25b6} "
        };
        let style = if selected {
            crate::theme::Theme::pulse_style(app.pulse_phase).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        items.push(ListItem::new(format!("{}{}", prefix, app_name)).style(style));
        render_idx += 1;

        if dropdown.app_dropdowns[i].expanded {
            for sub in &["  Edit Config", "  Delete App"] {
                let sub_selected = app.column_focus == 2 && dropdown.selected_option == render_idx;
                let sub_style = if sub_selected {
                    Style::default()
                        .fg(app.theme.warning)
                        .add_modifier(Modifier::BOLD)
                        .bg(Color::Rgb(60, 10, 10))
                } else {
                    Style::default().fg(app.theme.text)
                };
                items.push(ListItem::new(sub.to_string()).style(sub_style));
                render_idx += 1;
            }
        }
    }

    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ APPS ] ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(if app.column_focus == 2 {
                    app.theme.active_border_style()
                } else {
                    app.theme.border_style()
                }),
        ),
        cols[2],
    );

    f.render_widget(
        Paragraph::new(
            "  [n] new stack   [a] activate   [x] deactivate   [c] add core apps   [g/G] gpu on/off app   [s] deploy selected   [D/u] deploy/update all active",
        )
        .style(Style::default().fg(Color::DarkGray)),
        rows[1],
    );

    f.render_widget(
        Paragraph::new(format!("  status: {}", app.sync_status))
            .style(Style::default().fg(Color::Rgb(140, 160, 170))),
        rows[2],
    );
}

fn draw_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let total_apps: usize = app.stack_dropdowns.iter().map(|d| d.apps.len()).sum();

    // Vertical split: stat row (5 rows) + stack table (rest)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(area);

    // ── Stat boxes ────────────────────────────────────────────────────────
    let stat_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(rows[0]);

    f.render_widget(
        Paragraph::new(format!("\n  {}", app.stacks.len()))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" [ STACKS_TOTAL ] ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .style(app.theme.border_style()),
            ),
        stat_cols[0],
    );
    f.render_widget(
        Paragraph::new(format!("\n  {}", total_apps))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" [ CONTAINERS_TOTAL ] ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .style(app.theme.border_style()),
            ),
        stat_cols[1],
    );
    f.render_widget(
        Paragraph::new("\n  SSH agent \u{b7} branch: main [CLEAN]")
            .style(Style::default().fg(Color::Green))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" [ GIT_STATUS ] ")
                    .title_style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                    .style(app.theme.border_style()),
            ),
        stat_cols[2],
    );

    // ── Stack summary table ───────────────────────────────────────────────
    let header = Row::new(vec![
        Cell::from("STACK").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("APPS").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("PRE-SYNC").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .bottom_margin(1);

    let table_rows: Vec<Row> = app
        .stacks
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let app_count = app.stack_dropdowns[i].apps.len();
            let has_presync =
                std::path::Path::new(&format!("stacks/{}/pre-sync.sh", name)).exists();
            let presync_cell = if has_presync {
                Cell::from("  \u{2713} YES").style(Style::default().fg(Color::Green))
            } else {
                Cell::from("  \u{2717} no").style(Style::default().fg(Color::DarkGray))
            };
            Row::new(vec![
                Cell::from(name.as_str()).style(Style::default().fg(Color::White)),
                Cell::from(format!("  {}", app_count)).style(Style::default().fg(Color::White)),
                presync_cell,
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(50),
        Constraint::Length(6),
        Constraint::Length(10),
    ];
    f.render_widget(
        Table::new(table_rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" [ STACK_OVERVIEW ] ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .style(app.theme.border_style()),
            )
            .highlight_style(Style::default()),
        rows[1],
    );
}

fn draw_backups(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let schedule = &app.backup_schedule;
    let body = format!(
        "Backup Engine Policy (continuous service scheduler)\n\n\
Enabled:            {}\n\
Interval (minutes): {}\n\
Retention daily:    {}\n\
Retention weekly:   {}\n\
Retention monthly:  {}\n\
Notify success:     {}\n\
Notify failure:     {}\n",
        schedule.enabled,
        schedule.interval_minutes,
        schedule.retention_daily,
        schedule.retention_weekly,
        schedule.retention_monthly,
        schedule.notify_on_success,
        schedule.notify_on_failure
    );

    f.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ BACKUP POLICY ] ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(app.theme.border_style()),
        ),
        rows[0],
    );

    f.render_widget(
        Paragraph::new(
            "  [e] enabled  [+/-] interval  [d/D][w/W][m/M] retention  [n/f] notify  [s] save  [b] backup all  [i] restore stack  [r] full restore  [p] patch all  [u] unattended check",
        )
        .style(Style::default().fg(Color::DarkGray)),
        rows[1],
    );

    f.render_widget(
        Paragraph::new(format!("  status: {}", app.backup_status))
            .style(Style::default().fg(Color::Rgb(140, 160, 170))),
        rows[2],
    );
}

fn draw_host_management(f: &mut Frame, area: Rect, app: &App) {
    // ── Layout: banner (3) | main (fill) ────────────────────────────────────
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // ── Banner: >> HOST_MESH << ──────────────────────────────────────────────
    let node_count = app.stacks.len();
    let online_count = app
        .stacks
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 5 != 3)
        .count();
    let banner_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            ">> HOST_MESH <<",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("NODE: pve-01", Style::default().fg(Color::White)),
        Span::raw("  \u{b7}  "),
        Span::styled(
            "192.168.1.10",
            Style::default().fg(Color::Rgb(100, 150, 200)),
        ),
        Span::raw("  \u{b7}  "),
        Span::styled(
            format!("{}/{} ONLINE", online_count, node_count),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  \u{b7}  "),
        Span::styled("uptime: 47d 12h", Style::default().fg(Color::DarkGray)),
        Span::raw("  \u{b7}  "),
        Span::styled(
            "\u{26a0} LXC: MOCK DATA",
            Style::default().fg(Color::Yellow),
        ),
    ]);
    f.render_widget(
        Paragraph::new(banner_line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ PROXMOX_NODE :: pve-01 ] ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(app.theme.active_border_style()),
        ),
        v[0],
    );

    // ── Main split: LXC (60%) | SSH config (40%) ────────────────────────────
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(v[1]);

    // ── [ LXC_MESH ] ─────────────────────────────────────────────────────────
    let hdr_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let lxc_header = Row::new(vec![
        Cell::from("STATUS").style(hdr_style),
        Cell::from("ID").style(hdr_style),
        Cell::from("CONTAINER").style(hdr_style),
        Cell::from("IP").style(hdr_style),
        Cell::from("CPU").style(hdr_style),
        Cell::from("RAM").style(hdr_style),
        Cell::from("UPTIME").style(hdr_style),
    ])
    .bottom_margin(1);

    // Mock uptime: cyclic values for a realistic-looking mix.
    const MOCK_UPTIMES: &[&str] = &[
        "47d 12h", "12d  3h", " 3d 18h", " 0d  4h", "31d  0h", "47d 12h",
    ];

    let lxc_rows: Vec<Row> = app
        .stacks
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_selected = i == app.host_selected;
            let is_running = i % 5 != 3;
            let id = 101 + i;
            let ip = format!("192.168.1.{}", id);
            let container = crate::scaffold::read_stack_config(name)
                .map(|cfg| cfg.hostname)
                .unwrap_or_else(|_| crate::scaffold::legacy_lxc_alias(name));

            let (status_text, status_color) = if is_running {
                ("\u{25cf} RUN", Color::Green)
            } else {
                ("\u{25cb} STP", Color::DarkGray)
            };

            // CPU sparkline string from ring buffer (last 8 samples).
            let (cpu_spark, cpu_pct, cpu_color) = if let Some(d) = app.lxc_cpu.get(i) {
                let pct = d.back().copied().unwrap_or(0);
                let spark = mini_spark(d, 8);
                let color = load_color(pct);
                (spark, pct, color)
            } else {
                (String::from("        "), 0, Color::DarkGray)
            };

            let (ram_spark, ram_pct, ram_color) = if let Some(d) = app.lxc_ram.get(i) {
                let pct = d.back().copied().unwrap_or(0);
                let spark = mini_spark(d, 8);
                let color = load_color(pct);
                (spark, pct, color)
            } else {
                (String::from("        "), 0, Color::DarkGray)
            };

            let uptime = MOCK_UPTIMES[i % MOCK_UPTIMES.len()];

            // Selected row pulses; others use default style.
            let row_style = if is_selected {
                crate::theme::Theme::pulse_style(app.pulse_phase)
            } else if !is_running {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(id.to_string()).style(Style::default().fg(Color::White)),
                Cell::from(container).style(Style::default().fg(Color::White)),
                Cell::from(ip).style(Style::default().fg(Color::Rgb(100, 150, 200))),
                Cell::from(format!("{} {:2}%", cpu_spark, cpu_pct))
                    .style(Style::default().fg(cpu_color)),
                Cell::from(format!("{} {:2}%", ram_spark, ram_pct))
                    .style(Style::default().fg(ram_color)),
                Cell::from(uptime).style(Style::default().fg(Color::DarkGray)),
            ])
            .style(row_style)
        })
        .collect();

    let lxc_title = format!(
        " [ LXC_MESH :: {} NODES ]  [\u{2191}/\u{2193}] select ",
        node_count
    );
    f.render_widget(
        Table::new(
            lxc_rows,
            [
                Constraint::Length(7),  // STATUS
                Constraint::Length(4),  // ID
                Constraint::Min(16),    // CONTAINER
                Constraint::Length(15), // IP
                Constraint::Length(14), // CPU spark+pct
                Constraint::Length(14), // RAM spark+pct
                Constraint::Length(8),  // UPTIME
            ],
        )
        .header(lxc_header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .title(lxc_title.as_str())
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .style(app.theme.active_border_style()),
        )
        .highlight_style(Style::default()),
        cols[0],
    );

    // ── [ SSH_CONFIG :: LIVE ] ───────────────────────────────────────────────
    let ssh_entries = crate::ssh_config::parse_ssh_config();

    // Split SSH panel into: table (fill) | hint footer (1)
    let ssh_inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(cols[1]);

    let ssh_hdr_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);
    let ssh_header = Row::new(vec![
        Cell::from("ALIAS").style(ssh_hdr_style),
        Cell::from("HOST / IP").style(ssh_hdr_style),
        Cell::from("USER").style(ssh_hdr_style),
    ])
    .bottom_margin(1);

    let ssh_rows: Vec<Row> = if ssh_entries.is_empty() {
        vec![Row::new(vec![
            Cell::from("(empty)").style(Style::default().fg(Color::DarkGray)),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        ssh_entries
            .iter()
            .map(|e| {
                Row::new(vec![
                    Cell::from(e.host.as_str()).style(
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Cell::from(e.hostname.as_str())
                        .style(Style::default().fg(Color::Rgb(100, 150, 200))),
                    Cell::from(e.user.as_str()).style(Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect()
    };

    let ssh_title = if ssh_entries.is_empty() {
        " [ SSH_CONFIG :: EMPTY ] "
    } else {
        " [ SSH_CONFIG :: LIVE ] "
    };

    f.render_widget(
        Table::new(
            ssh_rows,
            [
                Constraint::Min(14),
                Constraint::Length(16),
                Constraint::Length(8),
            ],
        )
        .header(ssh_header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(ssh_title)
                .title_style(
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )
                .style(app.theme.modal_border_style()),
        )
        .highlight_style(Style::default()),
        ssh_inner[0],
    );

    f.render_widget(
        Paragraph::new("  [a] add / update alias   [Enter] connect")
            .style(Style::default().fg(Color::DarkGray)),
        ssh_inner[1],
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Renders the scrolling telemetry ticker at the bottom of the screen.
fn draw_ticker_bar(f: &mut Frame, area: Rect, app: &App) {
    let chars: Vec<char> = app.ticker_content.chars().collect();
    let total = chars.len();
    if total == 0 {
        return;
    }
    let width = area.width as usize;
    let offset = app.ticker_offset % total;
    let visible: String = (0..width).map(|i| chars[(offset + i) % total]).collect();
    f.render_widget(
        Paragraph::new(visible).style(Style::default().fg(Color::Rgb(45, 70, 65))),
        area,
    );
}

/// Maps a CPU/RAM percentage to a colour: green < 50, yellow < 80, red otherwise.
fn load_color(pct: u64) -> Color {
    if pct < 50 {
        Color::Green
    } else if pct < 80 {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Builds a mini Unicode block-character sparkline from the last `width` samples.
///
/// Maps 0–100 → ' ' `▁▂▃▄▅▆▇█` (9 levels).
fn mini_spark(data: &std::collections::VecDeque<u64>, width: usize) -> String {
    const BLOCKS: [char; 9] = [
        ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let start = data.len().saturating_sub(width);
    data.iter()
        .skip(start)
        .map(|&v| BLOCKS[((v * 8 / 100) as usize).min(8)])
        .collect()
}

fn draw_logs(f: &mut Frame, area: Rect, app: &App) {
    // ── Outer layout: header row (3) + log list (fill) + status footer (1) ──
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Header: Sources (scrollable) | Level filter ──────────────────────────
    // The level block has a fixed width; sources get everything else.
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(30)])
        .split(chunks[0]);

    // -- Sources block (horizontal scroll) -----------------------------------
    {
        let inner_w = header_cols[0].width.saturating_sub(4) as usize; // borders + 2 pad
        let has_left = app.log_source_scroll > 0;

        let mut spans: Vec<Span> = Vec::new();
        let mut used = 0usize;

        if has_left {
            spans.push(Span::styled(
                "\u{25c0} ",
                Style::default().fg(Color::Yellow),
            ));
            used += 2;
        }

        let mut last_end = app.log_source_scroll;
        for (name, color) in &LOG_SOURCES[app.log_source_scroll..] {
            // "+2": reserve space for " ▶" right-indicator if there are more after
            let label = format!("{} ", name);
            let reserve = if last_end + 1 < LOG_SOURCES.len() {
                2
            } else {
                0
            };
            if used + label.len() + reserve > inner_w {
                break;
            }
            spans.push(Span::styled(label.clone(), Style::default().fg(*color)));
            used += label.len();
            last_end += 1;
        }

        if last_end < LOG_SOURCES.len() {
            spans.push(Span::styled(
                " \u{25b6}",
                Style::default().fg(Color::Yellow),
            ));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Sources  [</> scroll] ")
                    .style(app.theme.border_style()),
            ),
            header_cols[0],
        );
    }

    // -- Level filter block ---------------------------------------------------
    {
        let active = app.log_level_filter;
        let mut spans: Vec<Span> = Vec::new();
        for filter in [
            LogLevelFilter::All,
            LogLevelFilter::Info,
            LogLevelFilter::Warn,
            LogLevelFilter::Error,
        ] {
            let label = format!(" {} ", filter.label());
            let style = if filter == active {
                let color = match filter {
                    LogLevelFilter::All => Color::Cyan,
                    LogLevelFilter::Info => Color::Green,
                    LogLevelFilter::Warn => Color::Yellow,
                    LogLevelFilter::Error => Color::Red,
                };
                Style::default()
                    .fg(Color::Black)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled(label, style));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Level [f] ")
                    .style(app.theme.border_style()),
            ),
            header_cols[1],
        );
    }

    // ── Scrollable log list ──────────────────────────────────────────────────
    let inner_height = chunks[1].height.saturating_sub(2) as usize;
    let scroll = app.log_scroll;

    // Apply level filter.
    let filtered: Vec<_> = app
        .logs
        .iter()
        .filter(|l| app.log_level_filter.matches(&l.level))
        .collect();

    let total = filtered.len();
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);

    let items: Vec<ListItem> = filtered[start..end]
        .iter()
        .map(|line| {
            let src_color = log_source_color(&line.source);
            let level_style = log_level_style(&line.level);
            let spans = vec![
                Span::styled(
                    format!(" {} ", line.time),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<18}", line.source),
                    Style::default().fg(src_color),
                ),
                Span::styled(format!("{:<6} ", line.level), level_style),
                Span::raw(line.message.as_str()),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = if scroll == 0 {
        " Logs [live] "
    } else {
        " Logs [paused \u{2014} End to resume] "
    };
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .title_style(if scroll == 0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                })
                .style(app.theme.border_style()),
        ),
        chunks[1],
    );

    // ── Footer: counts and hints ─────────────────────────────────────────────
    let all_total = app.logs.len();
    let status = if app.log_level_filter == LogLevelFilter::All {
        if scroll == 0 {
            format!(
                " {} lines  |  \u{2191}/\u{2193} scroll  |  </> source",
                all_total
            )
        } else {
            format!(
                " -{} from latest  |  End = resume  |  {}/{} lines",
                scroll, end, total
            )
        }
    } else {
        format!(
            " {} / {} lines  [filter: {}]  |  [f] cycle filter",
            total,
            all_total,
            app.log_level_filter.label()
        )
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

/// Maps a log source name to a display colour.
fn log_source_color(source: &str) -> Color {
    // Walk LOG_SOURCES so this stays in sync with the legend automatically.
    for (name, color) in LOG_SOURCES {
        if *name == source {
            return *color;
        }
    }
    Color::Gray
}

/// Maps a log level string to a display style.
fn log_level_style(level: &str) -> Style {
    match level {
        "WARN" => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        "ERROR" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::DarkGray),
    }
}
