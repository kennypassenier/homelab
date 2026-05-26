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

use crate::app::{App, Tab};
use crate::blast_radius::{
    ActiveModal, draw_app_creation_wizard, draw_delete_app_modal, draw_ssh_add_wizard, draw_warning_modal,
};

/// Renders the complete UI for the current frame.
pub fn draw_ui(f: &mut Frame, app: &App) {
    let size = f.size();

    // Vertical split: tab bar (3 rows) + body (rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(size);

    // ── Tab bar ─────────────────────────────────────────────────────────────
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
        ActiveModal::SshAddWizard(state) => {
            draw_ssh_add_wizard(f, size, state);
        }
        ActiveModal::None => match app.active_tab() {
            Tab::Scaffolding => draw_scaffolding(f, chunks[1], app),
            Tab::Dashboard => draw_dashboard(f, chunks[1], app),
            Tab::HostManagement => draw_host_management(f, chunks[1], app),
            Tab::Logs => draw_logs(f, chunks[1], app),
        },
    }
}

// ── Tab renderers ────────────────────────────────────────────────────────────

fn draw_scaffolding(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24), // stacks
            Constraint::Length(24), // actions
            Constraint::Min(0),     // apps
        ])
        .split(area);

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
                Style::default()
                    .fg(app.theme.accent_cyan)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
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
                .title("Stacks")
                .style(app.theme.border_style()),
        ),
        cols[0],
    );

    // Nothing else to render if there are no stacks yet
    if app.stacks.is_empty() {
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
                1 => Style::default().fg(app.theme.warning).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(app.theme.accent_cyan),
            };
            let style = if selected {
                base.add_modifier(Modifier::REVERSED)
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
                .title("Actions")
                .style(app.theme.border_style()),
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
            "▼ "
        } else {
            "▶ "
        };
        let style = if selected {
            Style::default()
                .fg(app.theme.accent_cyan)
                .add_modifier(Modifier::REVERSED)
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
                        .add_modifier(Modifier::REVERSED)
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
                .title("Applications")
                .style(app.theme.border_style()),
        ),
        cols[2],
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
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Stacks ")
                    .style(app.theme.border_style()),
            ),
        stat_cols[0],
    );
    f.render_widget(
        Paragraph::new(format!("\n  {}", total_apps))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Apps ")
                    .style(app.theme.border_style()),
            ),
        stat_cols[1],
    );
    f.render_widget(
        Paragraph::new("\n  SSH agent · branch: main")
            .style(Style::default().fg(Color::Green))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Git ")
                    .style(app.theme.border_style()),
            ),
        stat_cols[2],
    );

    // ── Stack summary table ───────────────────────────────────────────────
    let header = Row::new(vec![
        Cell::from("Stack").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("Apps").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("pre-sync").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
                Cell::from("  ✓").style(Style::default().fg(Color::Green))
            } else {
                Cell::from("  ✗").style(Style::default().fg(Color::DarkGray))
            };
            Row::new(vec![
                Cell::from(name.as_str()).style(Style::default().fg(Color::White)),
                Cell::from(format!("  {}", app_count))
                    .style(Style::default().fg(Color::White)),
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
                    .title(" Stack Overview ")
                    .style(app.theme.border_style()),
            )
            .highlight_style(Style::default()),
        rows[1],
    );
}

fn draw_host_management(f: &mut Frame, area: Rect, app: &App) {
    // Vertical: banner (1) + main content (fill)
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // ── Banner ─────────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(
            " NODE  pve-01 \u{b7} 192.168.1.10   \u{26a0}  LXC: MOCK \u{2014} SSH aliases: live from ~/.ssh/config",
        )
        .style(Style::default().fg(Color::Yellow)),
        v[0],
    );

    // Horizontal: LXC mock (55%) | SSH aliases (45%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(v[1]);

    // ── LXC Containers (mock) ───────────────────────────────────────────────
    let lxc_header = Row::new(vec![
        Cell::from("Status").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("ID").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("Container").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("IP").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])
    .bottom_margin(1);

    let lxc_rows: Vec<Row> = app
        .stacks
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let id = 101 + i;
            let ip = format!("192.168.1.{}", id);
            let container = format!("lxc-{}", name);
            let (status_text, status_color) = if i % 5 != 3 {
                ("\u{25cf} RUN", Color::Green)
            } else {
                ("\u{25cb} STP", Color::DarkGray)
            };
            Row::new(vec![
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(id.to_string()).style(Style::default().fg(Color::White)),
                Cell::from(container).style(Style::default().fg(Color::White)),
                Cell::from(ip).style(Style::default().fg(Color::White)),
            ])
        })
        .collect();

    f.render_widget(
        Table::new(
            lxc_rows,
            [
                Constraint::Length(7),
                Constraint::Length(5),
                Constraint::Min(18),
                Constraint::Length(16),
            ],
        )
        .header(lxc_header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" LXC Containers (mock) ")
                .style(app.theme.border_style()),
        )
        .highlight_style(Style::default()),
        cols[0],
    );

    // ── SSH Aliases (real data from ~/.ssh/config) ──────────────────────────
    let ssh_entries = crate::ssh_config::parse_ssh_config();

    let ssh_header = Row::new(vec![
        Cell::from("Host alias").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("IP").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Cell::from("User").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])
    .bottom_margin(1);

    let ssh_rows: Vec<Row> = if ssh_entries.is_empty() {
        vec![Row::new(vec![
            Cell::from("(no entries)").style(Style::default().fg(Color::DarkGray)),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        ssh_entries
            .iter()
            .map(|e| {
                Row::new(vec![
                    Cell::from(e.host.as_str()).style(Style::default().fg(Color::White)),
                    Cell::from(e.hostname.as_str()).style(Style::default().fg(Color::Cyan)),
                    Cell::from(e.user.as_str()).style(Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect()
    };

    let hint = if ssh_entries.is_empty() {
        " SSH Aliases  [a] add "
    } else {
        " SSH Aliases  [a] add / update "
    };

    f.render_widget(
        Table::new(
            ssh_rows,
            [Constraint::Min(18), Constraint::Length(16), Constraint::Length(8)],
        )
        .header(ssh_header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(hint)
                .style(app.theme.border_style()),
        )
        .highlight_style(Style::default()),
        cols[1],
    );
}

fn draw_logs(f: &mut Frame, area: Rect, app: &App) {
    // ── Outer layout: legend header (3) + log list (fill) + status footer (1) ──
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Source colour legend ─────────────────────────────────────────────────
    let legend_spans = vec![
        Span::styled("  lxc-cloudflared ", Style::default().fg(Color::Blue)),
        Span::styled("lxc-downloader ",    Style::default().fg(Color::Magenta)),
        Span::styled("lxc-gateway ",       Style::default().fg(Color::Yellow)),
        Span::styled("lxc-media ",         Style::default().fg(Color::Cyan)),
        Span::styled("lxc-monitoring ",    Style::default().fg(Color::Green)),
        Span::styled("lxc-paperless ",     Style::default().fg(Color::LightCyan)),
        Span::styled("lxc-vikunja ",       Style::default().fg(Color::LightMagenta)),
        Span::styled("HOST ",              Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("CLIENT",             Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ];
    f.render_widget(
        Paragraph::new(Line::from(legend_spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Sources ")
                .style(app.theme.border_style()),
        ),
        chunks[0],
    );

    // ── Scrollable log list ──────────────────────────────────────────────────
    // Available inner height inside the bordered block.
    let inner_height = chunks[1].height.saturating_sub(2) as usize;
    let total = app.logs.len();
    let scroll = app.log_scroll;

    // `end` is the first line below the viewport; `start` is the first visible.
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);

    let items: Vec<ListItem> = app.logs[start..end]
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
        " Logs [paused — End to resume] "
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

    // ── Footer: scroll indicator ─────────────────────────────────────────────
    let status = if scroll == 0 {
        format!(" {} lines  |  up/down to scroll", total)
    } else {
        format!(" -{} from latest  |  End = resume live  |  {}/{} lines", scroll, end, total)
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

/// Maps a log source name to a display colour.
fn log_source_color(source: &str) -> Color {
    match source {
        "lxc-cloudflared" => Color::Blue,
        "lxc-downloader"  => Color::Magenta,
        "lxc-gateway"     => Color::Yellow,
        "lxc-media"       => Color::Cyan,
        "lxc-monitoring"  => Color::Green,
        "lxc-paperless"   => Color::LightCyan,
        "lxc-vikunja"     => Color::LightMagenta,
        "HOST"            => Color::White,
        "CLIENT"          => Color::Cyan,
        _                 => Color::Gray,
    }
}

/// Maps a log level string to a display style.
fn log_level_style(level: &str) -> Style {
    match level {
        "WARN"  => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        "ERROR" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _       => Style::default().fg(Color::DarkGray),
    }
}
