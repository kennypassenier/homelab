//! Logs tab: the multiplexed live stream from HOST, colored per source stack.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::App;
use crate::fx;
use crate::sim::Level;
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let filter_label = app.log_filter.map(|l| l.label()).unwrap_or("ALL");
    let title = fx::glitch("LOG_STREAM", 0x106, app.tick, app.fx).unwrap_or_else(|| "LOG_STREAM".into());
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(title, THEME.title_active()),
            Span::styled(
                format!(" :: {} ", filter_label),
                Style::new().fg(THEME.yellow),
            ),
            Span::styled("] ", Style::new().fg(THEME.faint)),
        ]))
        .title(
            Line::from(vec![
                if app.logs_follow {
                    Span::styled("▶ FOLLOW ", THEME.ok())
                } else {
                    Span::styled("⏸ PAUSED ", THEME.warn())
                },
                Span::styled(
                    format!("{} lines ", app.world.logs.len()),
                    THEME.muted_style(),
                ),
            ])
            .right_aligned(),
        )
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let capacity = inner.height as usize;
    let filtered: Vec<&crate::sim::LogLine> = app
        .world
        .logs
        .iter()
        .filter(|l| app.log_filter.map(|f| l.level == f).unwrap_or(true))
        .collect();
    let start = filtered.len().saturating_sub(capacity);

    let scan = fx::scanline(inner.height, app.tick, 0x5CA9, app.fx);

    let lines: Vec<Line> = filtered[start..]
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let level_style = match l.level {
                Level::Debug => Style::new().fg(THEME.faint),
                Level::Info => Style::new().fg(THEME.blue),
                Level::Warn => THEME.warn(),
                Level::Error => THEME.err().add_modifier(Modifier::BOLD),
            };
            let src_color = match l.source.as_str() {
                "HOST" => THEME.text,
                "CLIENT" => THEME.cyan,
                other => THEME.stack_color(other),
            };
            let msg_style = match l.level {
                Level::Debug => Style::new().fg(THEME.muted),
                Level::Error => THEME.err(),
                Level::Warn => Style::new().fg(THEME.text),
                Level::Info => Style::new().fg(THEME.text),
            };
            let mut line = Line::from(vec![
                Span::styled(format!("{} ", l.time), Style::new().fg(THEME.faint)),
                Span::styled(format!("{} ", l.level.label()), level_style),
                Span::styled(
                    format!("{:<11}", l.source),
                    Style::new().fg(src_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(l.msg.clone(), msg_style),
            ]);
            if Some(i as u16) == scan {
                line = line.style(Style::new().bg(Color::Rgb(0x0D, 0x22, 0x26)));
            }
            line
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}
