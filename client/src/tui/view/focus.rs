//! Deploy focus window: a near-fullscreen takeover with the live task feed
//! (only this deploy's transcript) and the transfer visuals. Mockup-approved.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, Gauge, Paragraph};

use crate::tui::fx;
use crate::tui::model::{Focus, Model};
use crate::tui::theme::THEME;
use homelab_proto::LogLevel;

pub fn draw(f: &mut Frame, model: &Model, focus: &Focus) {
    let area = f.area();
    let rect = Rect {
        x: area.width / 12,
        y: 1,
        width: area.width - area.width / 6,
        height: area.height.saturating_sub(3),
    };
    f.render_widget(Clear, rect);

    let title = if focus.done {
        if focus.ok {
            format!("FOCUS :: {} :: COMPLETE", focus.title)
        } else {
            format!("FOCUS :: {} :: FAILED", focus.title)
        }
    } else {
        format!("FOCUS :: {} :: LIVE", focus.title)
    };
    let border = if focus.done && !focus.ok {
        THEME.border_danger()
    } else {
        THEME.border_modal()
    };
    let t = fx::glitch(&title, 0x30DA1, model.tick, model.fx).unwrap_or(title);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(border)
        .title(Line::from(vec![
            Span::styled(" >> ", Style::new().fg(THEME.faint)),
            Span::styled(
                t,
                Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" << ", Style::new().fg(THEME.faint)),
        ]))
        .title(
            Line::from(if focus.done {
                if focus.ok {
                    Span::styled(
                        "● ALL GATES PASSED ",
                        THEME.ok().add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(
                        "✗ INCIDENT BUNDLED ",
                        THEME.err().add_modifier(Modifier::BOLD),
                    )
                }
            } else {
                Span::styled(
                    format!("{} EXECUTING ", fx::spinner(model.tick)),
                    Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD),
                )
            })
            .right_aligned(),
        )
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows = Layout::vertical([
        Constraint::Min(4),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    // Task feed (only this deploy's transcript), scrollable, anchored.
    let capacity = rows[0].height as usize;
    let scroll = focus.scroll.min(focus.feed.len().saturating_sub(capacity));
    let end = focus.feed.len() - scroll;
    let start = end.saturating_sub(capacity);
    let lines: Vec<Line> = focus.feed[start..end]
        .iter()
        .map(|l| {
            let style = if l.msg.contains("[sync][run ]") {
                Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)
            } else if l.msg.contains("[sync][exit]") || l.msg.contains("[gate]") {
                THEME.ok()
            } else if l.msg.contains("[sync] Sync complete") {
                THEME.ok().add_modifier(Modifier::BOLD)
            } else {
                match l.level {
                    LogLevel::Error => THEME.err(),
                    LogLevel::Warn => THEME.warn(),
                    _ => Style::new().fg(THEME.muted),
                }
            };
            Line::from(Span::styled(format!("  {}", l.msg), style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), rows[0]);

    // T69: a step is waiting for a decision. It is drawn over the feed
    // rather than beside it, because the feed is exactly what the operator
    // is reading and a question elsewhere is a question missed. It carries
    // what each answer DOES, not only the two words — the same reason
    // Kenny's forms carry a consequences box (D82).
    if let Some(ask) = &model.pending_ask {
        let h = 9.min(rows[0].height);
        let box_rect = Rect {
            x: rows[0].x,
            y: rows[0].y + rows[0].height.saturating_sub(h),
            width: rows[0].width,
            height: h,
        };
        f.render_widget(Clear, box_rect);
        let inner_ask = Block::bordered()
            .border_type(BorderType::Double)
            .border_style(THEME.border_danger())
            .title(Line::from(Span::styled(
                format!(" ? {} :: {} ", ask.op, ask.step),
                THEME.warn().add_modifier(Modifier::BOLD),
            )))
            .style(Style::new().bg(THEME.elevated));
        let body = inner_ask.inner(box_rect);
        f.render_widget(inner_ask, box_rect);
        let q = vec![
            Line::from(Span::styled(
                format!("  {}", ask.what),
                Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  [a] toelaten  ", THEME.ok().add_modifier(Modifier::BOLD)),
                Span::styled(ask.if_allowed.clone(), Style::new().fg(THEME.muted)),
            ]),
            Line::from(vec![
                Span::styled("  [s] stoppen   ", THEME.err().add_modifier(Modifier::BOLD)),
                Span::styled(ask.if_stopped.clone(), Style::new().fg(THEME.muted)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  geen antwoord = onbeheerd; de stap gaat niet door",
                Style::new().fg(THEME.faint),
            )),
        ];
        f.render_widget(Paragraph::new(q), body);
    }

    // Transfer visuals for this deploy (G6).
    if let Some(t) = model.transfers.last() {
        let name: String = t
            .label
            .rsplit('/')
            .next()
            .unwrap_or(&t.label)
            .chars()
            .take(28)
            .collect();
        let flow_w = rows[1].width as usize;
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
        let tr = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(rows[1]);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("⇅ ", Style::new().fg(THEME.cyan)),
                Span::styled(name, Style::new().fg(THEME.cyan)),
                Span::styled(format!("  {}B", t.done), THEME.muted_style()),
            ])),
            tr[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                flow,
                Style::new().fg(THEME.magenta),
            ))),
            tr[1],
        );
    }

    // Progress-ish gauge from feed activity (indeterminate while live).
    let ratio = if focus.done {
        1.0
    } else {
        ((model.tick % 60) as f64 / 60.0).min(0.95)
    };
    let gauge = Gauge::default()
        .ratio(ratio)
        .gauge_style(
            Style::new()
                .fg(if focus.done && focus.ok {
                    THEME.green
                } else if focus.done {
                    THEME.red
                } else {
                    THEME.cyan
                })
                .bg(THEME.panel),
        )
        .label(Span::styled(
            if focus.done {
                focus
                    .result
                    .chars()
                    .take(inner.width as usize - 2)
                    .collect::<String>()
            } else {
                "streaming over TLS…".into()
            },
            Style::new().fg(THEME.text),
        ));
    f.render_widget(gauge, rows[2]);

    let footer = if focus.done {
        Line::from(vec![
            Span::styled("[ENTER]", THEME.hint()),
            Span::styled(" close  ", THEME.muted_style()),
            Span::styled("[UP/DOWN]", THEME.hint()),
            Span::styled(" review transcript", THEME.muted_style()),
        ])
    } else {
        Line::from(vec![
            Span::styled("[UP/DOWN]", THEME.hint()),
            Span::styled(" scroll  ", THEME.muted_style()),
            Span::styled("[ESC]", THEME.hint()),
            Span::styled(" background (deploy keeps running)", THEME.muted_style()),
        ])
    };
    f.render_widget(Paragraph::new(footer), rows[3]);
}
