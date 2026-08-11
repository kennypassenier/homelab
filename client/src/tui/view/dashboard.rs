//! Dashboard: HOST_MESH status, the LXC fleet table, and the live TRANSFER
//! panel (G6). Data is the real FleetState from the host.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, Gauge, Paragraph, Row, Table};

use super::panel_title;
use crate::tui::fx;
use crate::tui::model::Model;
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let cols = Layout::horizontal([Constraint::Min(50), Constraint::Length(34)]).split(area);
    let left = Layout::vertical([Constraint::Length(5), Constraint::Min(6)]).split(cols[0]);
    draw_host(f, model, left[0]);
    draw_fleet(f, model, left[1]);
    draw_transfers(f, model, cols[1]);
}

fn stack_color(name: &str) -> Color {
    THEME.stack_color(name)
}

fn draw_host(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(panel_title("HOST_MESH", 0x4057, model))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(fleet) = &model.fleet {
        let rows = Layout::vertical([Constraint::Length(1); 3]).split(inner);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● ", THEME.ok()),
                Span::styled(
                    fleet.host.name.clone(),
                    Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  [ONLINE]", THEME.ok()),
            ])),
            rows[0],
        );
        let disk_bar: String = {
            let filled = (fleet.host.disk_pct as usize * 24) / 100;
            format!("{}{}", "█".repeat(filled), "░".repeat(24 - filled))
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("SSD ", THEME.muted_style()),
                Span::styled(disk_bar, Style::new().fg(THEME.blue)),
                Span::styled(format!(" {}%", fleet.host.disk_pct), THEME.muted_style()),
            ])),
            rows[1],
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("TLS ", THEME.muted_style()),
                Span::styled(
                    fleet.host.tls_fingerprint.clone(),
                    Style::new().fg(THEME.green),
                ),
                Span::styled("  ", THEME.muted_style()),
                Span::styled(
                    fx::spinner(model.tick).to_string(),
                    Style::new().fg(THEME.cyan),
                ),
            ])),
            rows[2],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{} awaiting fleet state…", fx::spinner(model.tick)),
                THEME.muted_style(),
            ))),
            inner,
        );
    }
}

fn draw_fleet(f: &mut Frame, model: &Model, area: Rect) {
    let n = model.stack_count();
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(panel_title(
            &format!("LXC_MESH :: {} NODES", n),
            0x1E51,
            model,
        ))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fleet) = &model.fleet else { return };
    let header = Row::new(vec!["NODE", "STATUS", "APPS", "FLAGS"])
        .style(Style::new().fg(THEME.muted).add_modifier(Modifier::BOLD));
    let scan = fx::scanline(fleet.stacks.len() as u16, model.tick, 0x51, model.fx);

    let rows: Vec<Row> = fleet
        .stacks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let color = stack_color(&s.name);
            let (dot, st) = if s.online {
                ("●", THEME.ok())
            } else {
                ("○", THEME.muted_style())
            };
            let running = s.apps.iter().filter(|a| a.running).count();
            let mut flags = String::new();
            if s.drift {
                flags.push_str("[UPD] ");
            }
            if !s.env_sealed {
                flags.push_str("[NOENV]");
            }
            let mut row = Row::new(vec![
                Cell::from(Line::from(vec![
                    Span::styled("▎", Style::new().fg(color)),
                    Span::styled(
                        s.hostname.clone(),
                        Style::new().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ])),
                Cell::from(Line::from(vec![
                    Span::styled(format!("{} ", dot), st),
                    Span::styled(if s.online { "ONLINE" } else { "OFFLINE" }, st),
                ])),
                Cell::from(Span::styled(
                    format!("{}/{}", running, s.apps.len()),
                    if running == s.apps.len() {
                        THEME.ok()
                    } else {
                        THEME.warn()
                    },
                )),
                Cell::from(Span::styled(flags, THEME.warn())),
            ]);
            if Some(i as u16) == scan {
                row = row.style(Style::new().bg(Color::Rgb(0x0D, 0x22, 0x26)));
            }
            row
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(11),
            Constraint::Length(6),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(fx::pulse_bg(model.tick, model.fx))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Span::styled("▶ ", Style::new().fg(THEME.cyan)));
    let mut ts = ratatui::widgets::TableState::default();
    ts.select(Some(model.selected_stack));
    f.render_stateful_widget(table, inner, &mut ts);
}

fn draw_transfers(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_modal())
        .title(panel_title("DATA_TRANSFERS", 0x7A5F, model))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if model.transfers.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("no active transfers", THEME.muted_style())),
                Line::from(Span::styled(
                    "streams appear during deploy",
                    Style::new().fg(THEME.faint),
                )),
            ]),
            inner,
        );
        return;
    }

    let rows = Layout::vertical(
        model
            .transfers
            .iter()
            .map(|_| Constraint::Length(2))
            .collect::<Vec<_>>(),
    )
    .split(inner);
    for (t, rect) in model.transfers.iter().zip(rows.iter()) {
        let name: String = t
            .label
            .rsplit('/')
            .next()
            .unwrap_or(&t.label)
            .chars()
            .take(rect.width as usize)
            .collect();
        // Animated flow: ▸ characters marching along the stream.
        let flow_w = rect.width as usize;
        let head = (model.tick as usize) % flow_w.max(1);
        let flow: String = (0..flow_w)
            .map(|i| {
                if (i + head).is_multiple_of(4) {
                    '▸'
                } else {
                    '·'
                }
            })
            .collect();
        let r2 = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(*rect);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(name, Style::new().fg(THEME.cyan)),
                Span::styled(
                    match t.total {
                        Some(tot) => format!("  {}/{}B", t.done, tot),
                        None => format!("  {}B", t.done),
                    },
                    THEME.muted_style(),
                ),
            ])),
            r2[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                flow,
                Style::new().fg(THEME.magenta),
            ))),
            r2[1],
        );
    }
    let _ = Gauge::default();
}
