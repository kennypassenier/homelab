//! View layer (AR6): pure rendering from the model. The fx engine is a
//! separate stateless layer these views call — Elm structure governs state
//! flow, not how it looks.

mod dashboard;
mod doctor;
mod focus;
mod logs;
mod settings;
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
            Tab::Settings => settings::draw(f, model, rows[1]),
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
    if let Some(wiz) = &model.wizard {
        draw_wizard(f, model, wiz);
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

fn draw_wizard(f: &mut Frame, model: &Model, wiz: &crate::tui::model::Wizard) {
    use crate::tui::model::WizStep;
    let presets = &model.presets;
    let area = f.area();
    let w = 64u16.min(area.width - 4);
    let h = 18u16.min(area.height - 4);
    let rect = Rect {
        x: (area.width - w) / 2,
        y: area.height / 6,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let step_no = match wiz.step {
        WizStep::Preset => 1,
        WizStep::Name => 2,
        WizStep::Resources => 3,
        WizStep::Review => 4,
    };
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_modal())
        .title(Line::from(Span::styled(
            format!(" >> STACK_FORGE :: STEP {}/4 << ", step_no),
            Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(inner);

    // Breadcrumb.
    let crumbs = ["PRESET", "NAME", "RESOURCES", "REVIEW"];
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, c) in crumbs.iter().enumerate() {
        let active = i + 1 == step_no;
        spans.push(Span::styled(
            format!(" {} ", c),
            if active {
                Style::new()
                    .fg(THEME.bg)
                    .bg(THEME.cyan)
                    .add_modifier(Modifier::BOLD)
            } else if i + 1 < step_no {
                THEME.ok()
            } else {
                THEME.muted_style()
            },
        ));
        if i < crumbs.len() - 1 {
            spans.push(Span::styled(" ▶ ", Style::new().fg(THEME.faint)));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), rows[0]);

    match wiz.step {
        WizStep::Preset => {
            let lines: Vec<Line> = presets
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let sel = i == wiz.preset_idx;
                    let style = if sel {
                        Style::new()
                            .fg(THEME.cyan)
                            .bg(fx::pulse_bg(model.tick, model.fx))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(THEME.text)
                    };
                    Line::from(vec![
                        Span::styled(if sel { "▶ " } else { "  " }, style),
                        Span::styled(format!("{:<14}", p.name), style),
                        Span::styled(p.meta.description.clone(), THEME.muted_style()),
                    ])
                })
                .collect();
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizStep::Name => {
            let cursor = if (model.tick / 15).is_multiple_of(2) {
                "█"
            } else {
                " "
            };
            let vmid = crate::tui::model::next_free_vmid(model);
            let lines = vec![
                Line::from(Span::styled(
                    "stack name (lowercase, single word):",
                    THEME.muted_style(),
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("  λ ", Style::new().fg(THEME.cyan)),
                    Span::styled(
                        wiz.name.clone(),
                        Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(cursor, Style::new().fg(THEME.cyan)),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("  hostname → ", THEME.muted_style()),
                    Span::styled(
                        format!("{}-app-{}", vmid, wiz.name),
                        Style::new().fg(THEME.green),
                    ),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizStep::Resources => {
            use crate::tui::model::ResField;
            let cursor = if (model.tick / 15).is_multiple_of(2) {
                "█"
            } else {
                " "
            };
            let field = |sel: bool, label: &str, value: String, hint: &str| -> Line<'static> {
                let row_style = if sel {
                    Style::new().bg(fx::pulse_bg(model.tick, model.fx))
                } else {
                    Style::new()
                };
                let value_span = if sel {
                    Span::styled(
                        format!("‹ {} ›", value),
                        Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(format!("  {}  ", value), Style::new().fg(THEME.text))
                };
                Line::from(vec![
                    Span::styled(if sel { "▶ " } else { "  " }, Style::new().fg(THEME.cyan)),
                    Span::styled(format!("{:<6}", label), THEME.muted_style()),
                    value_span,
                    Span::styled(format!("   {}", hint), THEME.hint()),
                ])
                .style(row_style)
            };
            let ram_str = if wiz.ram >= 1024 {
                format!("{} GiB", wiz.ram / 1024)
            } else {
                format!("{} MiB", wiz.ram)
            };
            let disk_str = if wiz.res_field == ResField::Disk && wiz.disk_typing {
                format!("{}{} GiB", wiz.disk, cursor)
            } else {
                format!("{} GiB", wiz.disk)
            };
            let lines = vec![
                field(wiz.res_field == ResField::Ram, "RAM", ram_str, ""),
                field(
                    wiz.res_field == ResField::Cores,
                    "CPU",
                    format!("{} cores", wiz.cores),
                    "",
                ),
                field(
                    wiz.res_field == ResField::Disk,
                    "DISK",
                    disk_str,
                    "or type a size",
                ),
                field(
                    wiz.res_field == ResField::Swap,
                    "SWAP",
                    if wiz.swap == 0 {
                        "off".into()
                    } else {
                        format!("{} MiB", wiz.swap)
                    },
                    if wiz.swap_touched {
                        ""
                    } else {
                        "(auto from RAM)"
                    },
                ),
                field(
                    wiz.res_field == ResField::Vmid,
                    "VMID",
                    format!("{}", wiz.vmid),
                    &format!("→ ip .{}", wiz.vmid.saturating_sub(100)),
                ),
                Line::default(),
                Line::from(Span::styled(
                    format!(
                        "  ip 10.10.10.{}   order 99   protection on",
                        wiz.vmid.saturating_sub(100)
                    ),
                    Style::new().fg(THEME.faint),
                )),
                Line::from(vec![
                    Span::styled("  [UP/DOWN]", THEME.hint()),
                    Span::styled(" field   ", THEME.muted_style()),
                    Span::styled("[LEFT/RIGHT]", THEME.hint()),
                    Span::styled(" adjust", THEME.muted_style()),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizStep::Review => {
            let p = &presets[wiz.preset_idx];
            let vmid = wiz.vmid;
            let app = if p.apps.is_empty() {
                "(none)".to_string()
            } else {
                p.apps.join(", ")
            };
            let defaults = crate::scaffold::StackDefaults::default();
            let kv = |k: &str, v: String| -> Line<'static> {
                Line::from(vec![
                    Span::styled(format!("  {:<9}", k), THEME.muted_style()),
                    Span::styled(v, Style::new().fg(THEME.text)),
                ])
            };
            let lines = vec![
                Line::from(vec![
                    Span::styled("  name     ", THEME.muted_style()),
                    Span::styled(
                        wiz.name.clone(),
                        Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD),
                    ),
                ]),
                kv("hostname", format!("{}-app-{}", vmid, wiz.name)),
                kv(
                    "ip",
                    format!(
                        "{}{}/{}",
                        defaults.ip_prefix,
                        vmid.saturating_sub(100),
                        defaults.cidr
                    ),
                ),
                kv(
                    "resources",
                    format!(
                        "{} MiB · {} cores · {} GiB · swap {} MiB",
                        wiz.ram,
                        wiz.cores,
                        wiz.disk,
                        defaults.swap_for(wiz.ram)
                    ),
                ),
                kv("apps", format!("{}, promtail", app)),
                Line::default(),
                Line::from(Span::styled(
                    "  writes a real stacks/<name>/ tree; nothing deploys yet",
                    Style::new().fg(THEME.faint),
                )),
                Line::from(vec![
                    Span::styled("  ENTER ", THEME.hint()),
                    Span::styled("scaffold  (reversible: just delete the dir)", THEME.ok()),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[ENTER]", THEME.hint()),
            Span::styled(" next  ", THEME.muted_style()),
            Span::styled("[ESC]", THEME.hint()),
            Span::styled(" back/cancel  ", THEME.muted_style()),
            Span::styled("[UP/DOWN]", THEME.hint()),
            Span::styled(" select", THEME.muted_style()),
        ])),
        rows[2],
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

/// Attention-first status strip: surfaces only things that need action, then
/// falls back to calm live telemetry when everything is nominal. Every glimpse
/// is meaningful — no filler.
fn draw_ticker(f: &mut Frame, model: &Model, area: Rect) {
    let mut attn: Vec<String> = Vec::new();
    let mut calm: Vec<String> = Vec::new();

    if model.conn == Conn::Down {
        attn.push("⚠ LINK DOWN — reconnecting".into());
    }
    if let Some(fleet) = &model.fleet {
        let drifted: Vec<&str> = fleet
            .stacks
            .iter()
            .filter(|s| s.drift)
            .map(|s| s.name.as_str())
            .collect();
        if !drifted.is_empty() {
            attn.push(format!("⚠ UPD pending: {}", drifted.join(",")));
        }
        for s in fleet.stacks.iter().filter(|s| !s.env_sealed) {
            attn.push(format!("⚠ NOENV {} (deploy fails closed)", s.name));
        }
        for s in &fleet.stacks {
            let down: Vec<&str> = s
                .apps
                .iter()
                .filter(|a| !a.running)
                .map(|a| a.name.as_str())
                .collect();
            if !down.is_empty() {
                attn.push(format!("⚠ {} down in {}", down.join(","), s.name));
            }
        }
        // Active transfers are "in progress", worth surfacing.
        for t in &model.transfers {
            attn.push(format!(
                "⇅ {} {}B",
                t.label.rsplit('/').next().unwrap_or(&t.label),
                t.done
            ));
        }
        // Calm telemetry.
        let h = &fleet.host;
        calm.push(format!("{} up", h.name));
        calm.push(format!(
            "ram {}% used",
            (h.ram_used_mb as f64 / h.ram_total_mb.max(1) as f64 * 100.0) as u64
        ));
        calm.push(format!("load {:.2}", h.load1_x100 as f64 / 100.0));
        calm.push(format!("disk {}%", h.disk_pct));
        calm.push(format!("{} stacks", fleet.stacks.len()));
        calm.push("TLS pinned".into());
    }

    // If anything needs attention, show that (yellow); else calm (faint).
    let (segs, color) = if !attn.is_empty() {
        (attn, THEME.yellow)
    } else {
        let mut c = vec!["● ALL SYSTEMS NOMINAL".to_string()];
        c.extend(calm);
        (c, THEME.faint)
    };
    let text = fx::ticker_text(&segs, area.width, model.tick);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::new().fg(color).bg(THEME.bg),
        ))),
        area,
    );
}

fn draw_footer(f: &mut Frame, model: &Model, area: Rect) {
    // AZERTY: modifier names spelled out, digit-row hints shown as "1-4".
    let keys: &[(&str, &str)] = match model.tab {
        Tab::Dashboard | Tab::Stacks => &[
            ("1-5/TAB", "tabs"),
            ("N", "new stack"),
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
            ("1-5/TAB", "tabs"),
            ("CTRL+K", "palette"),
            ("Q", "quit"),
        ],
        Tab::Settings => &[
            ("UP/DOWN", "field"),
            ("LEFT/RIGHT", "value"),
            ("A", "add tier"),
            ("D", "del tier"),
            ("ENTER", "edit webhook"),
            ("SHIFT+S", "save"),
            ("R", "reload"),
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
