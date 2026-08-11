//! G4 SHELL tab: a line-based remote REPL over the audit-logged A6 exec
//! endpoint (deliberately not a full PTY — every command is one audited
//! round-trip). Requires `exec_enabled = true` in the host config; the
//! refusal message from the host is shown right here when it is off.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::tui::model::Model;
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let target = model
        .fleet
        .as_ref()
        .and_then(|fl| fl.stacks.get(model.shell_target))
        .map(|s| format!("{} ({})", s.hostname, s.vmid))
        .unwrap_or_else(|| "no stack".into());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" REMOTE_SHELL :: {} ", target))
        .title_style(THEME.title_active())
        .border_style(THEME.border_active());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);

    // Scrollback (tail).
    let visible = rows[0].height as usize;
    let start = model.shell_lines.len().saturating_sub(visible);
    let lines: Vec<Line> = model.shell_lines[start..]
        .iter()
        .map(|l| {
            if l.starts_with("▸ ") {
                Line::styled(l.clone(), Style::new().fg(THEME.cyan))
            } else if l.starts_with("exit 0") {
                Line::styled(l.clone(), THEME.ok())
            } else if l.starts_with("exit ") || l.contains("SAFETY ABORT") {
                Line::styled(l.clone(), THEME.err())
            } else {
                Line::styled(l.clone(), Style::new().fg(THEME.text))
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), rows[0]);

    // Prompt.
    let prompt = if model.shell_waiting {
        Line::styled("… waiting for host …", THEME.muted_style())
    } else {
        Line::from(vec![
            Span::styled("▸ ", Style::new().fg(THEME.cyan)),
            Span::styled(model.shell_input.clone(), Style::new().fg(THEME.text)),
            Span::styled("▏", Style::new().fg(THEME.cyan)),
        ])
    };
    let hint = Line::styled(
        "audited via A6 — needs exec_enabled = true in host.toml; no-touch vmids always refused",
        THEME.muted_style(),
    );
    f.render_widget(Paragraph::new(vec![prompt, hint]), rows[1]);
}
