//! Root renderer: chrome (tab bar, ticker, footer), flicker overlay, routing
//! to the active tab, modals and the command palette.

mod backups;
mod dashboard;
mod logs;
mod modals;
mod splash;
mod stacks;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Tabs};

use crate::app::{App, Modal, Screen, Tab};
use crate::fx::{self, FlickerPhase, FxLevel};
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    // Fill the whole canvas first.
    f.render_widget(Block::new().style(THEME.base()), area);

    if area.width < 80 || area.height < 24 {
        let msg = Paragraph::new(Line::from(Span::styled(
            format!("TERMINAL TOO SMALL — need 80x24, got {}x{}", area.width, area.height),
            THEME.err().add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        f.render_widget(msg, Rect { y: area.height / 2, height: 1, ..area });
        return;
    }

    if app.screen == Screen::Splash {
        splash::draw(f, app, area);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(3), // tab bar
        Constraint::Min(10),   // content
        Constraint::Length(1), // telemetry ticker
        Constraint::Length(1), // footer / keymap
    ])
    .split(area);

    draw_tab_bar(f, app, rows[0]);

    // Power-cycle flicker: dark ticks blank the content, flash tick brightens.
    match fx::flicker_phase(app.flicker) {
        FlickerPhase::Dark if app.fx != FxLevel::Off => {
            f.render_widget(Block::new().style(Style::new().bg(THEME.dim)), rows[1]);
        }
        FlickerPhase::Flash if app.fx != FxLevel::Off => {
            f.render_widget(Block::new().style(Style::new().bg(THEME.elevated)), rows[1]);
        }
        _ => {
            match app.tab {
                Tab::Dashboard => dashboard::draw(f, app, rows[1]),
                Tab::Stacks => stacks::draw(f, app, rows[1]),
                Tab::Backups => backups::draw(f, app, rows[1]),
                Tab::Logs => logs::draw(f, app, rows[1]),
            }
        }
    }

    draw_ticker(f, app, rows[2]);
    draw_footer(f, app, rows[3]);

    match &app.modal {
        Modal::None => {}
        _ => modals::draw(f, app),
    }
    if app.palette.open {
        draw_palette(f, app);
    }
}

fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .map(|t| {
            let label = t.title();
            let active = *t == app.tab;
            let styled = if active {
                let text = fx::glitch(label, 0xAB ^ t.index() as u64, app.tick, app.fx)
                    .unwrap_or_else(|| label.to_string());
                Span::styled(format!(" {} ", text), THEME.title_active())
            } else {
                Span::styled(format!(" {} ", label), THEME.title_inactive())
            };
            Line::from(styled)
        })
        .collect();

    let title = fx::glitch("HOMELAB :: CONTROL_DECK", 0xC0DE, app.tick, app.fx)
        .unwrap_or_else(|| "HOMELAB :: CONTROL_DECK".into());

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" ▓▒░ ", Style::new().fg(THEME.magenta)),
            Span::styled(title, THEME.title_active()),
            Span::styled(" ░▒▓ ", Style::new().fg(THEME.magenta)),
        ]))
        .title_alignment(Alignment::Left)
        .title(
            Line::from(vec![
                Span::styled("v2.0.0-mock ", THEME.muted_style()),
                Span::styled("● ", THEME.ok()),
                Span::styled("HOST_LINK", THEME.ok()),
                Span::styled(
                    format!(" {:.1}ms ", app.world.link_latency_ms),
                    THEME.muted_style(),
                ),
                Span::styled(app.fx.label(), Style::new().fg(THEME.yellow)),
                Span::raw(" "),
            ])
            .right_aligned(),
        )
        .style(THEME.panel_style());

    let inner = block.inner(area);
    f.render_widget(block, area);
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .divider(Span::styled("│", Style::new().fg(THEME.faint)));
    f.render_widget(tabs, inner);
}

fn draw_ticker(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.world;
    let segments: Vec<String> = vec![
        format!("0x{:04X}", (app.tick / 7) & 0xFFFF),
        format!("{} uptime={}", w.host.name, w.host.uptime),
        format!("cpu={:02}% ram={:02}%", w.host.cpu.last(), w.host.ram.last()),
        format!("temp={:.0}°C", w.host.temp),
        format!("disk={}%", w.host.disk_pct),
        format!("stacks {}/{} online", w.stacks.iter().filter(|s| s.enabled).count(), w.stacks.len()),
        format!("git {}@{}", w.git.branch, w.git.commit),
        "TLS PINNED".into(),
        "SYNC_OK".into(),
        "CRC_OK".into(),
    ];
    let text = fx::ticker_text(&segments, area.width, app.tick);
    let ticker = Paragraph::new(Line::from(Span::styled(
        text,
        Style::new().fg(THEME.faint).bg(THEME.bg),
    )));
    f.render_widget(ticker, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys: &[(&str, &str)] = match app.tab {
        Tab::Dashboard => &[
            ("1-4", "tabs"),
            ("j/k", "select"),
            ("↵", "open"),
            ("n", "new stack"),
            ("D", "deploy"),
            ("^k", "palette"),
            ("F2", "fx"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Tab::Stacks => &[
            ("j/k", "select"),
            ("n", "new"),
            ("D", "deploy"),
            ("a/x", "activate/deactivate"),
            ("b", "backup"),
            ("d", "delete"),
            ("^k", "palette"),
            ("q", "quit"),
        ],
        Tab::Backups => &[("j/k", "select"), ("b", "backup now"), ("^k", "palette"), ("q", "quit")],
        Tab::Logs => &[
            ("←/→", "source"),
            ("↑/↓", "scroll"),
            ("PgUp/Dn", "page"),
            ("G", "tail"),
            ("space", "follow"),
            ("f", "level"),
            ("^k", "palette"),
            ("q", "quit"),
        ],
    };
    let mut spans: Vec<Span> = Vec::new();
    for (k, desc) in keys {
        spans.push(Span::styled(format!("[{}]", k), THEME.hint()));
        spans.push(Span::styled(format!(" {}  ", desc), THEME.muted_style()));
    }
    let left = Line::from(spans);
    let right = Line::from(Span::styled(
        format!("{} ", app.status_line),
        Style::new().fg(THEME.blue),
    ))
    .right_aligned();
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

fn draw_palette(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = (area.width / 2).clamp(40, 64);
    let h = 14.min(area.height.saturating_sub(4));
    let rect = Rect {
        x: (area.width - w) / 2,
        y: area.height / 5,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_modal())
        .title(Line::from(Span::styled(
            " >> COMMAND_DECK << ",
            Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    let prompt = Line::from(vec![
        Span::styled("λ ", Style::new().fg(THEME.cyan)),
        Span::styled(app.palette.input.clone(), Style::new().fg(THEME.text)),
        Span::styled("█", Style::new().fg(if (app.tick / 15) % 2 == 0 { THEME.cyan } else { THEME.elevated })),
    ]);
    f.render_widget(Paragraph::new(prompt), rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::new().fg(THEME.faint),
        ))),
        rows[1],
    );

    let matches = app.palette.matches();
    let items: Vec<ListItem> = matches
        .iter()
        .enumerate()
        .map(|(i, &ai)| {
            let a = &crate::app::PALETTE_ACTIONS[ai];
            let style = if i == app.palette.selected {
                Style::new().fg(THEME.cyan).bg(fx::pulse_bg(app.tick, app.fx)).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(THEME.text)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if i == app.palette.selected { "▶ " } else { "  " }, style),
                Span::styled(a.label, style),
            ]))
        })
        .collect();
    f.render_widget(List::new(items), rows[2]);
}
