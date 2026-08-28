//! Log stream tab: the real multiplexed feed from HOST, with the source
//! selector (LEFT/RIGHT), arrow scrolling and anchored scrollback.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use super::panel_title;
use crate::tui::fx;
use crate::tui::model::Model;
use crate::tui::theme::THEME;
use homelab_proto::LogLevel;

fn source_name(model: &Model, idx: usize) -> String {
    if idx == 0 {
        "ALL".into()
    } else {
        model
            .fleet
            .as_ref()
            .and_then(|f| f.stacks.get(idx - 1))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "?".into())
    }
}

fn source_matches(model: &Model, source: &str) -> bool {
    if model.log_source == 0 {
        return true;
    }
    source_name(model, model.log_source) == source
}

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(panel_title("LOG_STREAM", 0x106, model))
        .title(
            Line::from(vec![
                if model.log_follow {
                    Span::styled("▶ FOLLOW ", THEME.ok())
                } else {
                    Span::styled(format!("⏸ SCROLL -{} ", model.log_scroll), THEME.warn())
                },
                Span::styled(format!("{} lines ", model.logs.len()), THEME.muted_style()),
            ])
            .right_aligned(),
        )
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);

    // Source selector bar.
    let n = model.stack_count() + 1;
    let mut spans: Vec<Span> = vec![Span::styled("◀ ", Style::new().fg(THEME.faint))];
    for i in 0..n {
        let name = source_name(model, i);
        let active = i == model.log_source;
        let color = if i == 0 {
            THEME.text
        } else {
            THEME.stack_color(&name)
        };
        let style = if active {
            Style::new()
                .fg(THEME.bg)
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(color)
        };
        spans.push(Span::styled(format!(" {} ", name.to_uppercase()), style));
        if i < n - 1 {
            spans.push(Span::styled("│", Style::new().fg(THEME.faint)));
        }
    }
    spans.push(Span::styled(" ▶  [LEFT/RIGHT] source", THEME.hint()));
    f.render_widget(Paragraph::new(Line::from(spans)), rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(rows[1].width as usize),
            Style::new().fg(THEME.faint),
        ))),
        rows[1],
    );

    let capacity = rows[2].height as usize;
    let filtered: Vec<&crate::tui::model::LogRow> = model
        .logs
        .iter()
        .filter(|l| source_matches(model, &l.source))
        .collect();
    let scroll = model
        .log_scroll
        .min(filtered.len().saturating_sub(capacity));
    let end = filtered.len() - scroll;
    let start = end.saturating_sub(capacity);
    let scan = fx::scanline(rows[2].height, model.tick, 0x5CA9, model.fx);

    let lines: Vec<Line> = filtered[start..end]
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let level_style = match l.level {
                LogLevel::Debug => Style::new().fg(THEME.faint),
                LogLevel::Info => Style::new().fg(THEME.blue),
                LogLevel::Warn => THEME.warn(),
                LogLevel::Error => THEME.err().add_modifier(Modifier::BOLD),
            };
            let src_color = match l.source.as_str() {
                "HOST" => THEME.text,
                other => THEME.stack_color(other),
            };
            let mut line = Line::from(vec![
                Span::styled(
                    format!("{:<6}", format!("{:?}", l.level).to_uppercase()),
                    level_style,
                ),
                Span::styled(
                    format!("{:<9}", l.source),
                    Style::new().fg(src_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(l.msg.clone(), Style::new().fg(THEME.text)),
            ]);
            if Some(i as u16) == scan {
                line = line.style(Style::new().bg(Color::Rgb(0x0D, 0x22, 0x26)));
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), rows[2]);
}
