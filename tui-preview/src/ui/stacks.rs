//! Stacks tab: stack list on the left, live detail (apps, manifest intent,
//! per-app state) on the right.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, List, ListItem, Paragraph, Row, Table};

use crate::app::App;
use crate::fx;
use crate::sim::{AppState, StackStatus};
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(40)]).split(area);
    draw_list(f, app, cols[0]);
    draw_detail(f, app, cols[1]);
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let title = fx::glitch("STACK_REGISTRY", 0x57AC, app.tick, app.fx)
        .unwrap_or_else(|| "STACK_REGISTRY".into());
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" >> ", Style::new().fg(THEME.faint)),
            Span::styled(title, THEME.title_active()),
            Span::styled(" << ", Style::new().fg(THEME.faint)),
        ]))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let reveal = app.reveal_progress();
    let items: Vec<ListItem> = app
        .world
        .stacks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let color = THEME.stack_color(&s.name);
            let selected = app.stack_table.selected() == Some(i);
            let dot = match s.status {
                StackStatus::Online => Span::styled("● ", THEME.ok()),
                StackStatus::Syncing => Span::styled(
                    format!("{} ", fx::spinner(app.tick)),
                    Style::new().fg(THEME.cyan),
                ),
                StackStatus::Degraded => Span::styled("◐ ", THEME.warn()),
                StackStatus::Offline => Span::styled("○ ", THEME.muted_style()),
            };
            let name = fx::decrypt(&s.hostname(), reveal, 0x100 + i as u64, app.tick);
            let mut spans = vec![
                Span::styled(if selected { "▶" } else { " " }, Style::new().fg(THEME.cyan)),
                dot,
                Span::styled(name, Style::new().fg(color).add_modifier(Modifier::BOLD)),
            ];
            if s.drift {
                spans.push(Span::styled(" [UPD]", THEME.warn()));
            }
            if !s.enabled {
                spans.push(Span::styled(" [OFF]", THEME.muted_style()));
            }
            let style = if selected {
                Style::new().bg(fx::pulse_bg(app.tick, app.fx))
            } else {
                Style::new()
            };
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let sel = app.selected_stack();
    if app.world.stacks.is_empty() {
        return;
    }
    let s = &app.world.stacks[sel];
    let color = THEME.stack_color(&s.name);

    let rows = Layout::vertical([Constraint::Length(6), Constraint::Min(6)]).split(area);

    // Intent / manifest summary panel.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(
                format!("MANIFEST :: {}", s.hostname()),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ] ", Style::new().fg(THEME.faint)),
        ]))
        .style(THEME.panel_style());
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);

    let enabled = if s.enabled {
        Span::styled("true", THEME.ok())
    } else {
        Span::styled("false", THEME.err())
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("vmid ", THEME.muted_style()),
            Span::styled(format!("{}", s.vmid), Style::new().fg(THEME.text)),
            Span::styled("   ip ", THEME.muted_style()),
            Span::styled(s.ip.clone(), Style::new().fg(THEME.text)),
            Span::styled("   deploy.enabled ", THEME.muted_style()),
            enabled,
        ]),
        Line::from(vec![
            Span::styled("ram ", THEME.muted_style()),
            Span::styled(
                format!("{} / {} MB", s.ram_mb, s.ram_limit_mb),
                Style::new().fg(THEME.text),
            ),
            Span::styled("   backup ", THEME.muted_style()),
            Span::styled(s.last_backup.clone(), Style::new().fg(THEME.text)),
            Span::styled("   env ", THEME.muted_style()),
            if s.sealed {
                Span::styled("● sealed", THEME.ok())
            } else {
                Span::styled("○ missing — deploy will fail closed", THEME.err())
            },
        ]),
        Line::from(vec![
            Span::styled("storage ", THEME.muted_style()),
            Span::styled(
                format!("/appdata/{}/<app>-config  →  /opt/{}/<app>-config", s.name, s.name),
                Style::new().fg(THEME.faint),
            ),
        ]),
        Line::from(vec![
            Span::styled("safety  ", THEME.muted_style()),
            Span::styled("whitelist ✓  hostname-guard ✓  fail-closed ✓", THEME.ok()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);

    // Apps table.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(
                format!("APP_GRID :: {} UNITS", s.apps.len()),
                THEME.title_active(),
            ),
            Span::styled(" ] ", Style::new().fg(THEME.faint)),
        ]))
        .style(THEME.panel_style());
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);

    let header = Row::new(vec!["APP", "IMAGE", "STATE", "CPU", "RST", "DIGEST"])
        .style(Style::new().fg(THEME.muted).add_modifier(Modifier::BOLD));

    let scan = fx::scanline(s.apps.len() as u16, app.tick, 0xA9 + sel as u64, app.fx);
    let table_rows: Vec<Row> = s
        .apps
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (label, style) = match a.state {
                AppState::Running => ("RUN ✓", THEME.ok()),
                AppState::Restarting => ("RESTART ⟳", THEME.warn()),
                AppState::Stopped => ("STOP ✗", THEME.err()),
            };
            let mut row = Row::new(vec![
                Cell::from(Span::styled(a.name, Style::new().fg(THEME.text).add_modifier(Modifier::BOLD))),
                Cell::from(Span::styled(a.image.clone(), THEME.muted_style())),
                Cell::from(Span::styled(label, style)),
                Cell::from(Span::styled(format!("{:>4.1}%", a.cpu), Style::new().fg(THEME.cyan))),
                Cell::from(Span::styled(
                    format!("{}", a.restarts),
                    if a.restarts > 0 { THEME.warn() } else { THEME.muted_style() },
                )),
                Cell::from(Span::styled(a.digest.clone(), Style::new().fg(THEME.faint))),
            ]);
            if Some(i as u16) == scan {
                row = row.style(Style::new().bg(Color::Rgb(0x0D, 0x22, 0x26)));
            }
            row
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(13),
            Constraint::Min(24),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Length(9),
        ],
    )
    .header(header);
    f.render_stateful_widget(table, inner, &mut app.app_table);
}
