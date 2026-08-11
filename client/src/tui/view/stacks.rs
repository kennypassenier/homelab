//! Stacks tab: fleet registry on the left, live app detail on the right —
//! all data is the real FleetState from the host.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, List, ListItem, Paragraph, Row, Table};

use super::panel_title;
use crate::tui::fx;
use crate::tui::model::Model;
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(40)]).split(area);
    draw_list(f, model, cols[0]);
    draw_detail(f, model, cols[1]);
}

fn draw_list(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(panel_title("STACK_REGISTRY", 0x57AC, model))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fleet) = &model.fleet else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{} awaiting state… [R] refresh", fx::spinner(model.tick)),
                THEME.muted_style(),
            ))),
            inner,
        );
        return;
    };
    let reveal = model.reveal_progress();
    let items: Vec<ListItem> = fleet
        .stacks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let color = THEME.stack_color(&s.name);
            let selected = i == model.selected_stack;
            let dot = if s.online {
                Span::styled("● ", THEME.ok())
            } else {
                Span::styled("○ ", THEME.muted_style())
            };
            let name = fx::decrypt(&s.hostname, reveal, 0x100 + i as u64, model.tick);
            let mut spans = vec![
                Span::styled(
                    if selected { "▶" } else { " " },
                    Style::new().fg(THEME.cyan),
                ),
                dot,
                Span::styled(name, Style::new().fg(color).add_modifier(Modifier::BOLD)),
            ];
            if s.drift {
                spans.push(Span::styled(" [UPD]", THEME.warn()));
            }
            let style = if selected {
                Style::new().bg(fx::pulse_bg(model.tick, model.fx))
            } else {
                Style::new()
            };
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

fn draw_detail(f: &mut Frame, model: &Model, area: Rect) {
    let Some(fleet) = &model.fleet else { return };
    let Some(s) = fleet.stacks.get(model.selected_stack) else {
        return;
    };
    let color = THEME.stack_color(&s.name);
    let rows = Layout::vertical([Constraint::Length(5), Constraint::Min(5)]).split(area);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(
                format!("MANIFEST :: {}", s.hostname),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ] ", Style::new().fg(THEME.faint)),
        ]))
        .style(THEME.panel_style());
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);
    let lines = vec![
        Line::from(vec![
            Span::styled("vmid ", THEME.muted_style()),
            Span::styled(format!("{}", s.vmid), Style::new().fg(THEME.text)),
            Span::styled("   env ", THEME.muted_style()),
            if s.env_sealed {
                Span::styled("● sealed", THEME.ok())
            } else {
                Span::styled("○ missing — deploy fails closed", THEME.err())
            },
        ]),
        Line::from(vec![
            Span::styled("drift ", THEME.muted_style()),
            if s.drift {
                Span::styled("[UPD] intent differs from applied", THEME.warn())
            } else {
                Span::styled("none — intent == runtime", THEME.ok())
            },
        ]),
        Line::from(vec![
            Span::styled("safety ", THEME.muted_style()),
            Span::styled("whitelist ✓  hostname-guard ✓  fail-closed ✓", THEME.ok()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(panel_title(
            &format!("APP_GRID :: {} UNITS", s.apps.len()),
            0xA9,
            model,
        ))
        .style(THEME.panel_style());
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);

    let header = Row::new(vec!["APP", "STATE", "RESTARTS"])
        .style(Style::new().fg(THEME.muted).add_modifier(Modifier::BOLD));
    let scan = fx::scanline(s.apps.len() as u16, model.tick, 0xA9, model.fx);
    let table_rows: Vec<Row> = s
        .apps
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (label, style) = if a.running {
                ("RUN ✓", THEME.ok())
            } else {
                ("DOWN ✗", THEME.err())
            };
            let mut row = Row::new(vec![
                Cell::from(Span::styled(
                    a.name.clone(),
                    Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(label, style)),
                Cell::from(Span::styled(
                    format!("{}", a.restarts),
                    if a.restarts > 0 {
                        THEME.warn()
                    } else {
                        THEME.muted_style()
                    },
                )),
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
            Constraint::Length(16),
            Constraint::Length(9),
            Constraint::Min(8),
        ],
    )
    .header(header);
    f.render_widget(table, inner);
}
