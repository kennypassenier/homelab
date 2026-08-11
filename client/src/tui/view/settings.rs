//! G8 SETTINGS tab: host runtime configuration, edited over the TLS line and
//! persisted to host.toml by the daemon. Self-contained: every field carries
//! its own explanation so no external docs are needed while editing.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::tui::model::Model;
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" HOST_SETTINGS ")
        .title_style(THEME.title_active())
        .border_style(THEME.border_active());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(cfg) = model.settings.as_ref() else {
        f.render_widget(
            Paragraph::new("requesting settings from host… (R to retry)")
                .style(THEME.muted_style()),
            inner,
        );
        return;
    };

    let sel = |row: usize| -> Style {
        if model.settings_row == row {
            Style::new()
                .fg(THEME.cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::new().fg(THEME.text)
        }
    };
    let webhook_row = 2 + cfg.retention.len() * 2 - 1;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::styled(
        "Changes apply live on the host after SHIFT+S (written to host.toml).",
        THEME.muted_style(),
    ));
    lines.push(Line::default());

    // Row 0: backup hour.
    let hour_label = match cfg.backup_hour {
        Some(h) => format!("{:02}:00", h),
        None => "off".into(),
    };
    lines.push(Line::from(vec![
        Span::styled("NIGHTLY RUN   ", THEME.hint()),
        Span::styled(format!("◂ {} ▸", hour_label), sel(0)),
        Span::styled(
            "   backup + auto-updates for every managed stack at this hour",
            THEME.muted_style(),
        ),
    ]));
    lines.push(Line::default());

    // Retention tiers.
    lines.push(Line::styled(
        "RETENTION — newest first; within each window one snapshot per interval is kept:",
        THEME.hint(),
    ));
    for (i, t) in cfg.retention.iter().enumerate() {
        let every_row = 1 + i * 2;
        let span_row = 2 + i * 2;
        let span_label = match t.span_days {
            Some(d) => format!("for {} days", d),
            None => "forever".into(),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  tier {}   ", i + 1), THEME.muted_style()),
            Span::styled("every ", Style::new().fg(THEME.text)),
            Span::styled(format!("◂ {}d ▸", t.every_days), sel(every_row)),
            Span::styled("  ", Style::new().fg(THEME.text)),
            Span::styled(format!("◂ {} ▸", span_label), sel(span_row)),
        ]));
    }
    lines.push(Line::styled(
        "  example: 1d/7d → 14d/60d → 60d/forever = daily week, biweekly 2 months, then bimonthly",
        THEME.muted_style(),
    ));
    lines.push(Line::default());

    // Webhook row.
    let hook = match model.settings_editing_webhook.as_ref() {
        Some(buf) => format!("{}▏ (ENTER save · ESC cancel)", buf),
        None => cfg
            .notify_webhook
            .clone()
            .unwrap_or_else(|| "off — ENTER to set".into()),
    };
    lines.push(Line::from(vec![
        Span::styled("WEBHOOK       ", THEME.hint()),
        Span::styled(hook, sel(webhook_row)),
    ]));
    lines.push(Line::styled(
        "  one POST per finished operation: {op, ok, error} — point it at a Home Assistant webhook",
        THEME.muted_style(),
    ));
    lines.push(Line::default());

    if model.settings_dirty {
        lines.push(Line::styled(
            "● unsaved changes — SHIFT+S to apply",
            THEME.warn(),
        ));
    } else {
        lines.push(Line::styled("● in sync with host", THEME.muted_style()));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
