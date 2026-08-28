//! Boot splash: ASCII logo materializing through a decrypt reveal, plus a
//! POST-style boot log. Any key skips straight to the dashboard.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::fx;
use crate::theme::THEME;

const LOGO: &[&str] = &[
    r"██╗  ██╗ ██████╗ ███╗   ███╗███████╗██╗      █████╗ ██████╗ ",
    r"██║  ██║██╔═══██╗████╗ ████║██╔════╝██║     ██╔══██╗██╔══██╗",
    r"███████║██║   ██║██╔████╔██║█████╗  ██║     ███████║██████╔╝",
    r"██╔══██║██║   ██║██║╚██╔╝██║██╔══╝  ██║     ██╔══██║██╔══██╗",
    r"██║  ██║╚██████╔╝██║ ╚═╝ ██║███████╗███████╗██║  ██║██████╔╝",
    r"╚═╝  ╚═╝ ╚═════╝ ╚═╝     ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝╚═════╝ ",
];

const BOOT_LINES: &[(&str, &str)] = &[
    ("SYS_CORE", "init sequence engaged"),
    ("CONFIG", "control deck profile loaded"),
    (
        "HOST_MESH",
        "link 10.10.5.250:8443 · TLS SHA256:9f2a…c41e [PINNED]",
    ),
    ("AUTH", "bearer token accepted · session opened"),
    ("GITOPS_ENGINE", "repo clean @ a3f9c21 · mirror ok"),
    ("SECRETS_VAULT", "4 stacks · all sealed · 0600"),
    ("TELEMETRY", "log stream online · 30 FPS render lock"),
    ("SAFETY", "whitelist armed · no-touch list enforced"),
    ("ALL SYSTEMS", "NOMINAL — press any key"),
];

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let content_h = (LOGO.len() + 2 + BOOT_LINES.len()) as u16;
    let top = area.height.saturating_sub(content_h) / 2;

    // Logo: reveal over the first ~1.5s.
    let logo_progress = (app.tick as f32 / 45.0).min(1.0);
    for (i, line) in LOGO.iter().enumerate() {
        let text = fx::decrypt(line, logo_progress, 0x1060 + i as u64, app.tick);
        let colored = if logo_progress >= 1.0 {
            // Subtle per-line gradient cyan → magenta.
            let t = i as f32 / (LOGO.len() - 1) as f32;
            let r = (0x00 as f32 + t * 0xFF as f32) as u8;
            let b = 0xFF;
            let g = (0xFF as f32 * (1.0 - t)) as u8;
            Span::styled(
                text,
                Style::new()
                    .fg(Color::Rgb(r, g, b))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(text, Style::new().fg(THEME.cyan))
        };
        let p = Paragraph::new(Line::from(colored)).alignment(Alignment::Center);
        f.render_widget(
            p,
            Rect {
                x: area.x,
                y: top + i as u16,
                width: area.width,
                height: 1,
            },
        );
    }

    // Boot log: one line lands every ~10 ticks after the logo.
    let boot_start = 45u64;
    let visible = if app.tick <= boot_start {
        0
    } else {
        (((app.tick - boot_start) / 10) as usize).min(BOOT_LINES.len())
    };
    for (i, (tag, msg)) in BOOT_LINES.iter().take(visible).enumerate() {
        let y = top + LOGO.len() as u16 + 2 + i as u16;
        let is_last = i == BOOT_LINES.len() - 1;
        let just_landed = i + 1 == visible;
        let reveal = if just_landed {
            (((app.tick - boot_start) % 10) as f32 / 6.0).min(1.0)
        } else {
            1.0
        };
        let tag_style = if is_last {
            THEME.ok().add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(THEME.cyan)
        };
        let text = fx::decrypt(msg, reveal, 0xB007 + i as u64, app.tick);
        let line = Line::from(vec![
            Span::styled("  ▸ ", Style::new().fg(THEME.magenta)),
            Span::styled(format!("{:<14}", tag), tag_style),
            Span::styled(":: ", Style::new().fg(THEME.faint)),
            Span::styled(
                text,
                if is_last {
                    THEME.ok()
                } else {
                    Style::new().fg(THEME.text)
                },
            ),
        ]);
        let p = Paragraph::new(line).alignment(Alignment::Center);
        f.render_widget(
            p,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
    }

    // Blinking skip hint at the very bottom.
    if (app.tick / 12).is_multiple_of(2) {
        let hint = Paragraph::new(Line::from(Span::styled(
            ">> INITIALIZING CONTROL_DECK — any key to skip <<",
            Style::new().fg(THEME.faint),
        )))
        .alignment(Alignment::Center);
        f.render_widget(
            hint,
            Rect {
                x: area.x,
                y: area.height.saturating_sub(2),
                width: area.width,
                height: 1,
            },
        );
    }
}
