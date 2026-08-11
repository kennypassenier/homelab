//! Boot splash — logo materializing through decrypt-reveal + a POST-style
//! boot log driven by the real connection state.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::tui::fx;
use crate::tui::model::{Conn, Model};
use crate::tui::theme::THEME;

const LOGO: &[&str] = &[
    r"██╗  ██╗ ██████╗ ███╗   ███╗███████╗██╗      █████╗ ██████╗ ",
    r"██║  ██║██╔═══██╗████╗ ████║██╔════╝██║     ██╔══██╗██╔══██╗",
    r"███████║██║   ██║██╔████╔██║█████╗  ██║     ███████║██████╔╝",
    r"██╔══██║██║   ██║██║╚██╔╝██║██╔══╝  ██║     ██╔══██║██╔══██╗",
    r"██║  ██║╚██████╔╝██║ ╚═╝ ██║███████╗███████╗██║  ██║██████╔╝",
    r"╚═╝  ╚═╝ ╚═════╝ ╚═╝     ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝╚═════╝ ",
];

pub fn draw(f: &mut Frame, model: &Model, area: Rect) {
    let content_h = (LOGO.len() + 2 + 6) as u16;
    let top = area.height.saturating_sub(content_h) / 2;

    let logo_progress = (model.tick as f32 / 40.0).min(1.0);
    for (i, line) in LOGO.iter().enumerate() {
        let text = fx::decrypt(line, logo_progress, 0x1060 + i as u64, model.tick);
        let colored = if logo_progress >= 1.0 {
            let t = i as f32 / (LOGO.len() - 1) as f32;
            let r = (t * 0xFF as f32) as u8;
            let g = (0xFF as f32 * (1.0 - t)) as u8;
            Span::styled(
                text,
                Style::new()
                    .fg(Color::Rgb(r, g, 0xFF))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(text, Style::new().fg(THEME.cyan))
        };
        f.render_widget(
            Paragraph::new(Line::from(colored)).alignment(Alignment::Center),
            Rect {
                x: area.x,
                y: top + i as u16,
                width: area.width,
                height: 1,
            },
        );
    }

    // Boot log reflects the real link state.
    let link = match model.conn {
        Conn::Up => ("HOST_MESH", "link established · TLS pinned", THEME.ok()),
        Conn::Connecting => (
            "HOST_MESH",
            "negotiating TLS…",
            Style::new().fg(THEME.yellow),
        ),
        Conn::Down => ("HOST_MESH", "link down — check host/token", THEME.err()),
    };
    let boot: Vec<(&str, &str, Style)> = vec![
        (
            "SYS_CORE",
            "control deck online",
            Style::new().fg(THEME.cyan),
        ),
        link,
        (
            "SAFETY",
            "whitelist armed · no-touch enforced",
            Style::new().fg(THEME.cyan),
        ),
        (
            "READY",
            "press any key",
            THEME.ok().add_modifier(Modifier::BOLD),
        ),
    ];
    let visible = ((model.tick / 12) as usize).min(boot.len());
    for (i, (tag, msg, style)) in boot.iter().take(visible).enumerate() {
        let y = top + LOGO.len() as u16 + 2 + i as u16;
        let line = Line::from(vec![
            Span::styled("  ▸ ", Style::new().fg(THEME.magenta)),
            Span::styled(format!("{:<12}", tag), *style),
            Span::styled(":: ", Style::new().fg(THEME.faint)),
            Span::styled(msg.to_string(), *style),
        ]);
        f.render_widget(
            Paragraph::new(line).alignment(Alignment::Center),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
    }
}
