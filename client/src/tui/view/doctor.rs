//! Doctor tab (F6): renders the host's self-diagnosis with health coloring.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use super::panel_title;
use crate::tui::fx;
use crate::tui::model::Model;
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(panel_title("SELF_DIAGNOSIS", 0xD0C, model))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if model.doctor_text.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("{} running checks…", fx::spinner(model.tick)),
                    THEME.muted_style(),
                )),
                Line::from(Span::styled(
                    "press R or ENTER to re-run",
                    Style::new().fg(THEME.faint),
                )),
            ]),
            inner,
        );
        return;
    }

    let reveal = model.reveal_progress();
    let lines: Vec<Line> = model
        .doctor_text
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let style = if raw.contains("[Fail]") {
                THEME.err().add_modifier(Modifier::BOLD)
            } else if raw.contains("[Warn]") {
                THEME.warn()
            } else if raw.contains("[Ok]") {
                THEME.ok()
            } else if raw.trim_start().starts_with('↳') {
                Style::new().fg(THEME.yellow)
            } else {
                Style::new().fg(THEME.text).add_modifier(Modifier::BOLD)
            };
            let text = fx::decrypt(raw, reveal, 0xD0C + i as u64, model.tick);
            Line::from(Span::styled(text, style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
