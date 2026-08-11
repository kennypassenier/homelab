//! View layer (AR6): pure rendering from the model. The fx engine is a
//! separate stateless layer these views call — Elm structure governs state
//! flow, not how it looks.

mod dashboard;
mod doctor;
mod focus;
mod logs;
mod splash;
mod stacks;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Tabs};

use crate::tui::fx::{self, FlickerPhase, FxLevel};
use crate::tui::model::{palette_matches, Conn, Model, Screen, Tab, PALETTE};
use crate::tui::theme::THEME;

pub fn draw(f: &mut Frame, model: &Model) {
    let area = f.area();
    f.render_widget(Block::new().style(THEME.base()), area);

    if area.width < 80 || area.height < 24 {
        let msg = Paragraph::new(Line::from(Span::styled(
            format!(
                "TERMINAL TOO SMALL — need 80x24, got {}x{}",
                area.width, area.height
            ),
            THEME.err().add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        f.render_widget(
            msg,
            Rect {
                y: area.height / 2,
                height: 1,
                ..area
            },
        );
        return;
    }

    if model.screen == Screen::Splash {
        splash::draw(f, model, area);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_tab_bar(f, model, rows[0]);

    match fx::flicker_phase(model.flicker) {
        FlickerPhase::Dark if model.fx != FxLevel::Off => {
            f.render_widget(Block::new().style(Style::new().bg(THEME.dim)), rows[1]);
        }
        FlickerPhase::Flash if model.fx != FxLevel::Off => {
            f.render_widget(Block::new().style(Style::new().bg(THEME.elevated)), rows[1]);
        }
        _ => match model.tab {
            Tab::Dashboard => dashboard::draw(f, model, rows[1]),
            Tab::Stacks => stacks::draw(f, model, rows[1]),
            Tab::Logs => logs::draw(f, model, rows[1]),
            Tab::Doctor => doctor::draw(f, model, rows[1]),
        },
    }

    draw_ticker(f, model, rows[2]);
    draw_footer(f, model, rows[3]);

    // Focus window (deploy) overlays everything.
    if let Some(fc) = &model.focus {
        focus::draw(f, model, fc);
    }
    if let Some(plan) = &model.plan {
        draw_plan(f, model, plan);
    }
    if model.help_open {
        draw_help(f);
    }
    if model.palette_open {
        draw_palette(f, model);
    }
}

fn draw_plan(f: &mut Frame, model: &Model, plan: &crate::tui::model::Plan) {
    let area = f.area();
    let w = 72u16.min(area.width - 4);
    let h = (plan.lines.len() as u16 + 5).min(area.height - 4);
    let rect = Rect {
        x: (area.width - w) / 2,
        y: area.height / 6,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let title = fx::glitch(
        &format!("CHANGE_PLAN :: {}", plan.stack),
        0xB1A5,
        model.tick,
        model.fx,
    )
    .unwrap_or_else(|| format!("CHANGE_PLAN :: {}", plan.stack));
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_modal())
        .title(Line::from(Span::styled(
            format!(" >> {} << ", title),
            Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(inner);
    let lines: Vec<Line> = plan
        .lines
        .iter()
        .map(|(sign, text)| {
            let (prefix, style) = match sign {
                '+' => ("+ ", THEME.ok()),
                '-' => ("- ", THEME.err()),
                '~' => ("~ ", THEME.warn()),
                _ => ("  ", Style::new().fg(THEME.text)),
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(text.clone(), style),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[ENTER]", THEME.hint()),
            Span::styled(" execute deploy   ", THEME.muted_style()),
            Span::styled("[ESC]", THEME.hint()),
            Span::styled(" cancel", THEME.muted_style()),
        ])),
        rows[1],
    );
}

fn draw_tab_bar(f: &mut Frame, model: &Model, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .map(|t| {
            let label = t.title();
            if *t == model.tab {
                let text = fx::glitch(label, 0xAB ^ t.index() as u64, model.tick, model.fx)
                    .unwrap_or_else(|| label.to_string());
                Line::from(Span::styled(format!(" {} ", text), THEME.title_active()))
            } else {
                Line::from(Span::styled(format!(" {} ", label), THEME.title_inactive()))
            }
        })
        .collect();

    let title = fx::glitch("HOMELAB :: CONTROL_DECK", 0xC0DE, model.tick, model.fx)
        .unwrap_or_else(|| "HOMELAB :: CONTROL_DECK".into());

    let (dot, conn_txt, conn_style) = match model.conn {
        Conn::Up => ("● ", "HOST_LINK", THEME.ok()),
        Conn::Connecting => ("◍ ", "LINKING", Style::new().fg(THEME.yellow)),
        Conn::Down => ("○ ", "LINK_DOWN", THEME.err()),
    };

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" ▓▒░ ", Style::new().fg(THEME.magenta)),
            Span::styled(title, THEME.title_active()),
            Span::styled(" ░▒▓ ", Style::new().fg(THEME.magenta)),
        ]))
        .title(
            Line::from(vec![
                Span::styled(dot, conn_style),
                Span::styled(conn_txt, conn_style),
                Span::styled(
                    format!(" {} ", model.fx.label()),
                    Style::new().fg(THEME.yellow),
                ),
            ])
            .right_aligned(),
        )
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let tabs = Tabs::new(titles)
        .select(model.tab.index())
        .divider(Span::styled("│", Style::new().fg(THEME.faint)));
    f.render_widget(tabs, inner);
}

fn draw_ticker(f: &mut Frame, model: &Model, area: Rect) {
    let mut segs: Vec<String> = vec![format!("0x{:04X}", (model.tick / 7) & 0xFFFF)];
    if let Some(fleet) = &model.fleet {
        segs.push(format!("{} disk={}%", fleet.host.name, fleet.host.disk_pct));
        segs.push(format!("stacks {}", fleet.stacks.len()));
        segs.push(format!("TLS {}", short_fp(&fleet.host.tls_fingerprint)));
    }
    segs.push(match model.conn {
        Conn::Up => "LINK_OK".into(),
        Conn::Connecting => "LINKING".into(),
        Conn::Down => "LINK_DOWN".into(),
    });
    for t in &model.transfers {
        segs.push(format!(
            "⇅ {} {}B",
            t.label.rsplit('/').next().unwrap_or(&t.label),
            t.done
        ));
    }
    let text = fx::ticker_text(&segs, area.width, model.tick);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::new().fg(THEME.faint).bg(THEME.bg),
        ))),
        area,
    );
}

fn short_fp(fp: &str) -> String {
    fp.chars().take(11).collect()
}

fn draw_footer(f: &mut Frame, model: &Model, area: Rect) {
    // AZERTY: modifier names spelled out, digit-row hints shown as "1-4".
    let keys: &[(&str, &str)] = match model.tab {
        Tab::Dashboard | Tab::Stacks => &[
            ("1-4/TAB", "tabs"),
            ("UP/DOWN", "select"),
            ("P", "plan"),
            ("SHIFT+D", "deploy"),
            ("R", "refresh"),
            ("CTRL+K", "palette"),
            ("H", "help"),
            ("Q", "quit"),
        ],
        Tab::Logs => &[
            ("LEFT/RIGHT", "source"),
            ("UP/DOWN", "scroll"),
            ("SPACE", "follow"),
            ("G", "tail"),
            ("CTRL+K", "palette"),
            ("Q", "quit"),
        ],
        Tab::Doctor => &[
            ("R/ENTER", "re-run"),
            ("1-4/TAB", "tabs"),
            ("CTRL+K", "palette"),
            ("Q", "quit"),
        ],
    };
    let mut spans: Vec<Span> = Vec::new();
    for (k, d) in keys {
        spans.push(Span::styled(format!("[{}]", k), THEME.hint()));
        spans.push(Span::styled(format!(" {}  ", d), THEME.muted_style()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    f.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                format!("{} ", model.status_line),
                Style::new().fg(THEME.blue),
            ))
            .right_aligned(),
        ),
        area,
    );
}

fn draw_help(f: &mut Frame) {
    let area = f.area();
    let w = 60u16.min(area.width - 4);
    let h = 16u16.min(area.height - 4);
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
            " >> KEYMAP << ",
            Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows: &[(&str, &str)] = &[
        ("1-4 or & é \" '", "switch tab (AZERTY-safe)"),
        ("TAB / SHIFT+TAB", "cycle tabs"),
        ("UP / DOWN", "move selection / scroll"),
        ("LEFT / RIGHT", "log source (Logs tab)"),
        ("SPACE", "follow logs"),
        ("R", "refresh fleet state"),
        ("CTRL+K", "command palette"),
        ("F2", "cycle effect intensity"),
        ("H", "this help"),
        ("Q", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {:<16}", k), THEME.hint()),
                Span::styled(v.to_string(), Style::new().fg(THEME.text)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_palette(f: &mut Frame, model: &Model) {
    let area = f.area();
    let w = (area.width / 2).clamp(40, 60);
    let h = 13u16.min(area.height - 4);
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
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);
    let cursor = if (model.tick / 15).is_multiple_of(2) {
        "█"
    } else {
        " "
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("λ ", Style::new().fg(THEME.cyan)),
            Span::styled(model.palette_input.clone(), Style::new().fg(THEME.text)),
            Span::styled(cursor, Style::new().fg(THEME.cyan)),
        ])),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::new().fg(THEME.faint),
        ))),
        rows[1],
    );
    let matches = palette_matches(&model.palette_input);
    let items: Vec<ListItem> = matches
        .iter()
        .enumerate()
        .map(|(i, &ai)| {
            let sel = i == model.palette_sel;
            let style = if sel {
                Style::new()
                    .fg(THEME.cyan)
                    .bg(fx::pulse_bg(model.tick, model.fx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(THEME.text)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if sel { "▶ " } else { "  " }, style),
                Span::styled(PALETTE[ai].label, style),
            ]))
        })
        .collect();
    f.render_widget(List::new(items), rows[2]);
}

/// Shared panel title helper.
pub fn panel_title(text: &str, id: u64, model: &Model) -> Line<'static> {
    let t = fx::glitch(text, id, model.tick, model.fx).unwrap_or_else(|| text.to_string());
    Line::from(vec![
        Span::styled(" [ ", Style::new().fg(THEME.faint)),
        Span::styled(t, THEME.title_active()),
        Span::styled(" ] ", Style::new().fg(THEME.faint)),
    ])
}
