//! Logs tab: the multiplexed live stream from HOST, colored per source stack.
//! ←/→ walks the source selector (ALL → HOST → CLIENT → each stack), j/k and
//! PgUp/PgDn scroll history, G/End snaps back to the live tail.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::app::App;
use crate::fx;
use crate::sim::Level;
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let filter_label = app.log_filter.map(|l| l.label()).unwrap_or("ALL");
    let title =
        fx::glitch("LOG_STREAM", 0x106, app.tick, app.fx).unwrap_or_else(|| "LOG_STREAM".into());
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(title, THEME.title_active()),
            Span::styled(
                format!(" :: LVL {} ", filter_label),
                Style::new().fg(THEME.yellow),
            ),
            Span::styled("] ", Style::new().fg(THEME.faint)),
        ]))
        .title(
            Line::from(vec![
                if app.logs_follow {
                    Span::styled("▶ FOLLOW ", THEME.ok())
                } else {
                    Span::styled(format!("⏸ SCROLL -{} ", app.log_scroll), THEME.warn())
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

    let rows = Layout::vertical([
        Constraint::Length(1), // source selector
        Constraint::Length(1), // divider
        Constraint::Min(1),    // stream
    ])
    .split(inner);

    draw_source_bar(f, app, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(rows[1].width as usize),
            Style::new().fg(THEME.faint),
        ))),
        rows[1],
    );
    draw_stream(f, app, rows[2]);
}

fn draw_source_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled("◀ ", Style::new().fg(THEME.faint))];
    let n = app.log_source_count();
    for i in 0..n {
        let name = app.log_source_name(i);
        let active = i == app.log_source;
        let base_color = match i {
            0 => THEME.text,
            1 => THEME.text,
            2 => THEME.cyan,
            _ => THEME.stack_color(&name),
        };
        let style = if active {
            Style::new()
                .fg(THEME.bg)
                .bg(base_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(base_color)
        };
        let label = if active {
            format!(" ▣ {} ", name.to_uppercase())
        } else {
            format!(" {} ", name.to_uppercase())
        };
        spans.push(Span::styled(label, style));
        if i < n - 1 {
            spans.push(Span::styled("│", Style::new().fg(THEME.faint)));
        }
    }
    spans.push(Span::styled(" ▶", Style::new().fg(THEME.faint)));
    spans.push(Span::styled("  [←/→] source", THEME.hint()));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_stream(f: &mut Frame, app: &mut App, area: Rect) {
    let capacity = area.height as usize;
    let filtered: Vec<&crate::sim::LogLine> = app
        .world
        .logs
        .iter()
        .filter(|l| app.log_filter.map(|f| l.level == f).unwrap_or(true))
        .filter(|l| app.log_source_matches(&l.source))
        .collect();

    // Clamp scroll so we can't run off the top of the buffer.
    let max_scroll = filtered.len().saturating_sub(capacity);
    if app.log_scroll > max_scroll {
        app.log_scroll = max_scroll;
    }
    let end = filtered.len() - app.log_scroll.min(filtered.len());
    let start = end.saturating_sub(capacity);

    let scan = fx::scanline(area.height, app.tick, 0x5CA9, app.fx);

    let lines: Vec<Line> = filtered[start..end]
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

    f.render_widget(Paragraph::new(lines), area);

    // Scrollbar reflecting the window position within the filtered history.
    if filtered.len() > capacity {
        let mut sb_state = ScrollbarState::new(filtered.len().saturating_sub(capacity))
            .position(start)
            .viewport_content_length(capacity);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(THEME.cyan))
            .track_style(Style::new().fg(THEME.faint))
            .begin_symbol(None)
            .end_symbol(None);
        f.render_stateful_widget(sb, area, &mut sb_state);
    }
}
