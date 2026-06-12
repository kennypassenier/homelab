//! All terminal rendering logic for the homelab client TUI.
//!
//! The single public entry point is `draw_ui`, called unconditionally every
//! iteration of the event loop before polling for input.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
    },
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

    // ── Tab bar with cyberpunk glitch effects ───────────────────────────────
    let tab_titles: Vec<String> = Tab::all()
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let title = t.title();
            if idx == app.active_tab {
                apply_glitch_effect(title, app.pulse_phase * 0.18 + (idx as f32 * 0.37), 0.06)
            } else {
                apply_glitch_effect(title, app.pulse_phase * 0.12 + (idx as f32 * 0.21), 0.03)
            }
        })
        .collect();
    let tab_titles_static: Vec<&str> = tab_titles.iter().map(|s| s.as_str()).collect();

    let pulse_strength = app.pulse_phase.sin() * 0.5 + 0.5;
    let tab_bar_style = if pulse_strength > 0.6 {
        app.theme.active_border_style().add_modifier(Modifier::BOLD)
    } else {
        app.theme.active_border_style()
    };

    let tabs = Tabs::new(tab_titles_static)
        .select(app.active_tab)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .title(apply_glitch_effect(
                    " [ SYS_CORE :: CLIENT ] ",
                    app.pulse_phase * 0.1 + 1.37,
                    0.02,
                ))
                .style(tab_bar_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(Color::Rgb(100, 110, 130)));
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
            Tab::Update => draw_update(f, root[1], app),
            Tab::Logs => draw_logs(f, root[1], app),
        },
    }

    // ── Telemetry ticker (bottom row, always visible) ────────────────────────
    draw_ticker_bar(f, root[2], app);
}

// ── Tab renderers ────────────────────────────────────────────────────────────

fn stack_is_active(stack_name: &str) -> bool {
    crate::scaffold::is_stack_deploy_enabled(stack_name).unwrap_or(false)
}

fn single_line(text: &str) -> String {
    text.replace('\n', " | ").replace('\r', " ")
}

fn slice_chars(text: &str, offset: usize, max_chars: usize) -> String {
    text.chars().skip(offset).take(max_chars).collect()
}

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
            let is_active = stack_is_active(name);
            let selected = i == app.selected_stack && app.column_focus == 0;
            let style = if selected {
                crate::theme::Theme::pulse_style(app.pulse_phase).add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let state_tag = if is_active { "[ON ]" } else { "[OFF]" };
            ListItem::new(format!(" {} {}", state_tag, name)).style(style)
        })
        .collect();

    f.render_widget(
        List::new(stack_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ STACKS :: ON/OFF ] ")
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
        Paragraph::new(format!("  status: {}", single_line(&app.sync_status)))
            .style(Style::default().fg(Color::Rgb(140, 160, 170))),
        rows[2],
    );
}

fn draw_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let total_apps: usize = app.stack_dropdowns.iter().map(|d| d.apps.len()).sum();
    let (git_status_text, git_status_color) = live_git_status_text();

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
        Paragraph::new(format!("\n  {}", git_status_text))
            .style(Style::default().fg(git_status_color))
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
        Cell::from("STATE").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
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
            let is_active = stack_is_active(name);
            let app_count = app.stack_dropdowns[i].apps.len();
            let has_presync =
                std::path::Path::new(&format!("stacks/{}/pre-sync.sh", name)).exists();
            let state_cell = if is_active {
                Cell::from("  ON").style(Style::default().fg(Color::Green))
            } else {
                Cell::from(" OFF").style(Style::default().fg(Color::DarkGray))
            };
            let presync_cell = if has_presync {
                Cell::from("  \u{2713} YES").style(Style::default().fg(Color::Green))
            } else {
                Cell::from("  \u{2717} no").style(Style::default().fg(Color::DarkGray))
            };
            Row::new(vec![
                state_cell,
                Cell::from(name.as_str()).style(Style::default().fg(if is_active {
                    Color::White
                } else {
                    Color::DarkGray
                })),
                Cell::from(format!("  {}", app_count)).style(Style::default().fg(Color::White)),
                presync_cell,
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(7),
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
        Paragraph::new(format!("  status: {}", single_line(&app.backup_status)))
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
        .host_lxc_runtime
        .iter()
        .filter(|row| row.status.eq_ignore_ascii_case("running"))
        .count();
    let host_status = if app.host_connected {
        Span::styled(
            "HOST: ONLINE",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "HOST: OFFLINE",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    };
    let banner_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            ">> HOST_MESH <<",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("NODE: {}", app.host_node_name),
            Style::default().fg(Color::White),
        ),
        Span::raw("  \u{b7}  "),
        Span::styled(
            &app.host_node_ip,
            Style::default().fg(Color::Rgb(100, 150, 200)),
        ),
        Span::raw("  \u{b7}  "),
        host_status,
        Span::raw("  \u{b7}  "),
        Span::styled(
            format!("{}/{} ONLINE", online_count, node_count),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  \u{b7}  "),
        Span::styled(
            format!("uptime: {}", app.host_uptime),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(
        Paragraph::new(banner_line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" [ PROXMOX_NODE ] ")
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

    let lxc_rows: Vec<Row> = app
        .stacks
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_selected = i == app.host_selected;
            let container = crate::scaffold::read_stack_config(name)
                .map(|cfg| cfg.hostname)
                .unwrap_or_else(|_| crate::scaffold::legacy_lxc_alias(name));

            let runtime = app.host_lxc_runtime.iter().find(|row| {
                row.name == container
                    || row.name.ends_with(&crate::scaffold::legacy_lxc_alias(name))
                    || row.name.ends_with(name)
            });

            let is_running = runtime
                .map(|row| row.status.eq_ignore_ascii_case("running"))
                .unwrap_or(false);
            let id = runtime
                .map(|row| row.vmid.to_string())
                .unwrap_or_else(|| "--".to_string());

            let ip = if let Ok(cfg) = crate::scaffold::read_stack_config(name) {
                if let Some(reserved) = cfg.reserved_ipv4.filter(|v| !v.trim().is_empty()) {
                    reserved
                } else {
                    let env_key = format!("LXC_{}_IP", name.replace('-', "_").to_uppercase());
                    std::env::var(&env_key)
                        .ok()
                        .filter(|v| !v.trim().is_empty())
                        .unwrap_or_else(|| "unknown".to_string())
                }
            } else {
                "unknown".to_string()
            };

            let (status_text, status_color) = if is_running {
                ("\u{25cf} RUN", Color::Green)
            } else {
                ("\u{25cb} STP", Color::DarkGray)
            };

            let cpu_text = if runtime.is_some() { "--" } else { "n/a" };
            let ram_text = if runtime.is_some() { "--" } else { "n/a" };
            let uptime = if let Some(row) = runtime {
                format_duration(row.uptime_secs)
            } else {
                "n/a".to_string()
            };

            let cpu_pct = runtime.map(|row| row.cpu_pct);
            let ram_pct = runtime.map(|row| row.ram_pct);
            let cpu_cell = cpu_pct
                .map(|value| format!("{:>3}%", value))
                .unwrap_or_else(|| cpu_text.to_string());
            let ram_cell = ram_pct
                .map(|value| format!("{:>3}%", value))
                .unwrap_or_else(|| ram_text.to_string());

            let cpu_color = load_color(cpu_pct.unwrap_or(0));
            let ram_color = load_color(ram_pct.unwrap_or(0));

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
                Cell::from(id).style(Style::default().fg(Color::White)),
                Cell::from(container).style(Style::default().fg(Color::White)),
                Cell::from(ip).style(Style::default().fg(Color::Rgb(100, 150, 200))),
                Cell::from(cpu_cell).style(Style::default().fg(cpu_color)),
                Cell::from(ram_cell).style(Style::default().fg(ram_color)),
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

    if !app.host_connected && !app.host_last_error.is_empty() {
        let warn = Rect {
            x: cols[0].x,
            y: cols[0].y.saturating_add(cols[0].height.saturating_sub(1)),
            width: cols[0].width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(format!("HOST probe error: {}", app.host_last_error))
                .style(Style::default().fg(Color::Red)),
            warn,
        );
    }
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

fn live_git_status_text() -> (String, Color) {
    let Ok(repo) = git2::Repository::discover(".") else {
        return ("git unavailable".to_string(), Color::DarkGray);
    };

    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(ToString::to_string))
        .unwrap_or_else(|| "detached".to_string());

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let dirty = repo
        .statuses(Some(&mut opts))
        .map(|statuses| !statuses.is_empty())
        .unwrap_or(false);

    if dirty {
        (format!("branch: {} [DIRTY]", branch), Color::Yellow)
    } else {
        (format!("branch: {} [CLEAN]", branch), Color::Green)
    }
}

fn load_color(percent: u8) -> Color {
    if percent < 50 {
        Color::Green
    } else if percent < 80 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn format_duration(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;

    if days > 0 {
        format!("{}d {:02}h", days, hours)
    } else if hours > 0 {
        format!("{}h {:02}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn draw_update(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(7),
            Constraint::Length(6),
        ])
        .split(area);

    let header = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" DAEMON UPDATE CONTROL ")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let instructions = vec![
        Line::from(Span::raw(
            "Auto-update checks every 30 minutes. Manual trigger forces immediate update.",
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled(
                "[1/h] ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("HOST daemon  "),
            Span::styled(
                "[2-9] ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("individual LXC stacks  "),
            Span::styled(
                "[a] ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("UPDATE ALL"),
        ]),
    ];
    f.render_widget(Paragraph::new(instructions).block(header), layout[0]);

    let total_targets = (app.stacks.len() + 1).max(1);
    let button_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Ratio(1, total_targets as u32);
            total_targets
        ])
        .split(layout[1]);

    let mut col_idx = 0;
    let is_updating = app.update_in_progress.is_some();
    let spinner = if is_updating {
        match (app.pulse_phase * 4.0) as usize % 4 {
            0 => "◐",
            1 => "◓",
            2 => "◑",
            _ => "◒",
        }
    } else {
        " "
    };

    if col_idx < button_cols.len() {
        let btn_style = if app.update_in_progress == Some("HOST".to_string()) {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let btn_text = format!("{} HOST UPDATE {}", spinner, spinner);
        let host_state = if app.update_in_progress == Some("HOST".to_string()) {
            "updating"
        } else {
            "idle"
        };
        let host_last = app
            .update_last_result
            .get("HOST")
            .map(String::as_str)
            .unwrap_or("not run yet");
        let host_last_at = app
            .update_last_at
            .get("HOST")
            .map(String::as_str)
            .unwrap_or("n/a");
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(" key ", Style::default().fg(Color::Cyan)),
                    Span::raw("[1/h]"),
                    Span::raw("   "),
                    Span::styled("target ", Style::default().fg(Color::Cyan)),
                    Span::raw("HOST"),
                ]),
                Line::from(vec![
                    Span::styled(" running ", Style::default().fg(Color::Green)),
                    Span::raw(app.host_daemon_version.clone()),
                ]),
                Line::from(vec![
                    Span::styled(" available ", Style::default().fg(Color::LightMagenta)),
                    Span::raw(app.host_latest_release.clone()),
                ]),
                Line::from(vec![
                    Span::styled(" checked ", Style::default().fg(Color::DarkGray)),
                    Span::raw(app.host_latest_checked_at.clone()),
                    Span::raw("   "),
                    Span::styled("state ", Style::default().fg(Color::Cyan)),
                    Span::raw(host_state),
                ]),
                Line::from(vec![
                    Span::styled(" last ", Style::default().fg(Color::Yellow)),
                    Span::raw(host_last),
                ]),
                Line::from(vec![
                    Span::styled(" at ", Style::default().fg(Color::DarkGray)),
                    Span::raw(host_last_at),
                ]),
            ])
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(btn_style)
                    .title(btn_text),
            ),
            button_cols[col_idx],
        );
        col_idx += 1;
    }

    for (stack_idx, stack) in app.stacks.iter().enumerate() {
        if col_idx >= button_cols.len() {
            break;
        }
        let btn_style = if app.update_in_progress == Some(stack.clone()) {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Magenta)
        };
        let btn_text = format!("{} {} {}", spinner, stack.to_uppercase(), spinner);
        let source_key = format!("lxc-{}", stack);
        let stack_state = if app.update_in_progress == Some(stack.clone()) {
            "updating"
        } else {
            "idle"
        };
        let stack_last = app
            .update_last_result
            .get(stack)
            .map(String::as_str)
            .unwrap_or("not run yet");
        let stack_last_at = app
            .update_last_at
            .get(stack)
            .map(String::as_str)
            .unwrap_or("n/a");
        let lxc_available = if app.lxc_update_channel.contains(":latest") {
            "rolling latest".to_string()
        } else {
            app.lxc_update_channel.clone()
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(" key ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("[{}]", stack_idx + 2)),
                    Span::raw("   "),
                    Span::styled("target ", Style::default().fg(Color::Cyan)),
                    Span::raw(source_key.clone()),
                ]),
                Line::from(vec![
                    Span::styled(" running ", Style::default().fg(Color::Green)),
                    Span::raw(
                        app.lxc_daemon_versions
                            .get(&source_key)
                            .map(String::as_str)
                            .unwrap_or("unknown"),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(" available ", Style::default().fg(Color::LightMagenta)),
                    Span::raw(lxc_available),
                ]),
                Line::from(vec![
                    Span::styled(" state ", Style::default().fg(Color::Cyan)),
                    Span::raw(stack_state),
                ]),
                Line::from(vec![
                    Span::styled(" last ", Style::default().fg(Color::Yellow)),
                    Span::raw(stack_last),
                ]),
                Line::from(vec![
                    Span::styled(" at ", Style::default().fg(Color::DarkGray)),
                    Span::raw(stack_last_at),
                ]),
            ])
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(btn_style)
                    .title(btn_text),
            ),
            button_cols[col_idx],
        );
        col_idx += 1;
    }

    let footer_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(10)])
        .split(layout[2]);

    let update_all_style = if app.update_in_progress.is_some() {
        Style::default().fg(Color::Gray)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from("[a] trigger batch"),
            Line::from("HOST -> all stacks"),
            Line::from(format!("latest host {}", app.host_latest_release)),
            Line::from(
                app.update_last_result
                    .get("UPDATING_ALL")
                    .map(String::as_str)
                    .unwrap_or("not run yet"),
            ),
        ])
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(update_all_style)
                .title(" >>> UPDATE ALL <<< "),
        ),
        footer_cols[0],
    );

    let status_text = if let Some(in_prog) = &app.update_in_progress {
        format!("Updating {} ...", in_prog)
    } else if app.update_status.is_empty() {
        "Ready".to_string()
    } else {
        single_line(&app.update_status)
    };

    let status_color =
        if app.update_status.contains("failed") || app.update_status.contains("error") {
            Color::Red
        } else if app.update_status.contains("success") {
            Color::Green
        } else {
            Color::Yellow
        };

    let status_block = Block::default()
        .borders(Borders::ALL)
        .title(" Status ")
        .style(Style::default().fg(status_color));
    f.render_widget(
        Paragraph::new(status_text)
            .wrap(Wrap { trim: true })
            .block(status_block),
        footer_cols[1],
    );
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
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(30)])
        .split(chunks[0]);

    // -- Sources block (horizontal scroll) -----------------------------------
    {
        let inner_w = header_cols[0].width.saturating_sub(4) as usize;
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
        for (idx, (name, color)) in LOG_SOURCES.iter().enumerate().skip(app.log_source_scroll) {
            let label = format!("{} ", name);
            let reserve = if last_end + 1 < LOG_SOURCES.len() {
                2
            } else {
                0
            };
            if used + label.len() + reserve > inner_w {
                break;
            }
            let style = if app.log_source_selected == idx {
                Style::default().fg(*color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(*color)
            };
            spans.push(Span::styled(label, style));
            used += name.len() + 1;
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
                    .title(" Sources  [</> select] [Shift+f focus] ")
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
    let focused_source = app.focused_source();

    let filtered: Vec<_> = app
        .logs
        .iter()
        .filter(|l| app.log_level_filter.matches(&l.level))
        .filter(|l| {
            focused_source
                .map(|source| l.source == source)
                .unwrap_or(true)
        })
        .collect();

    let total = filtered.len();
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    let inner_log_width = chunks[1].width.saturating_sub(2) as usize;
    let prefix_width = 10 + 18 + 7;
    let msg_width = inner_log_width.saturating_sub(prefix_width).max(1);

    let items: Vec<ListItem> = filtered[start..end]
        .iter()
        .map(|line| {
            let src_color = log_source_color(&line.source);
            let level_style = log_level_style(&line.level);
            let msg = single_line(&line.message);
            let visible_msg = slice_chars(&msg, app.log_hscroll, msg_width);
            let mut spans = vec![
                Span::styled(
                    format!(" {} ", line.time),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<18}", line.source),
                    Style::default().fg(src_color),
                ),
                Span::styled(format!("{:<6} ", line.level), level_style),
            ];
            spans.extend(styled_log_message(visible_msg));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = if scroll == 0 {
        if let Some(source) = focused_source {
            format!(" Logs [live | focus:{}] ", source)
        } else {
            " Logs [live] ".to_string()
        }
    } else {
        " Logs [paused \u{2014} End to resume] ".to_string()
    };
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title.as_str())
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
    let focus_suffix = focused_source
        .map(|source| format!("  |  focus={}", source))
        .unwrap_or_default();
    let status = if app.log_level_filter == LogLevelFilter::All {
        if scroll == 0 {
            format!(
                " {} lines  |  \u{2191}/\u{2193} scroll  |  h/l line-pan  |  </> source{}",
                all_total, focus_suffix
            )
        } else {
            format!(
                " -{} from latest  |  End = resume  |  hscroll={}  |  {}/{} lines{}",
                scroll, app.log_hscroll, end, total, focus_suffix
            )
        }
    } else {
        format!(
            " {} / {} lines  [filter: {}]  |  [f] cycle filter  |  h/l line-pan{}",
            total,
            all_total,
            app.log_level_filter.label(),
            focus_suffix
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

fn styled_log_message(message: String) -> Vec<Span<'static>> {
    let tags = [
        ("[host-update]", Color::Cyan),
        ("[failsafe]", Color::Yellow),
        ("[update-rpc]", Color::Magenta),
        ("[update-http]", Color::Blue),
        ("[sync]", Color::Green),
    ];

    for (tag, color) in tags {
        if let Some(rest) = message.strip_prefix(tag) {
            return vec![
                Span::styled(tag, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Span::raw(rest.to_string()),
            ];
        }
    }

    vec![Span::raw(message)]
}

/// Applies subtle cyberpunk glitch effects to text.
/// Randomly mutates characters into symbols at the specified threshold.
/// Returns the text with occasional character replacements for a brief glitch effect.
fn apply_glitch_effect(text: &str, phase: f32, glitch_rate: f32) -> String {
    const GLITCH_CHARS: &[char] = &[
        '◆', '▲', '■', '✧', '⚡', '✹', '◈', '▬', '⬓', '◐', '◑', '◒', '◓', '◀', '▶', '▼', '░', '▓',
        '█', '◊', '●', '◯', '◎', '◉', '⬟',
    ];

    let seed = (phase.sin().abs() * 10000.0) as u32;
    let mut hash = seed;

    text.chars()
        .map(|ch| {
            hash = hash.wrapping_mul(1103515245).wrapping_add(12345);
            let rand_val = ((hash / 65536) % 100) as f32 / 100.0;

            if ch.is_alphanumeric() && rand_val < glitch_rate {
                let glyph_idx = ((hash / 65536) as usize) % GLITCH_CHARS.len();
                GLITCH_CHARS[glyph_idx]
            } else {
                ch
            }
        })
        .collect()
}
