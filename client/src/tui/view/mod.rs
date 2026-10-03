//! View layer (AR6): pure rendering from the model. The fx engine is a
//! separate stateless layer these views call — Elm structure governs state
//! flow, not how it looks.

mod dashboard;
mod doctor;
mod focus;
mod logs;
mod shell;
mod splash;
mod stacks;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Tabs};

use crate::tui::fx::{self, FlickerPhase, FxLevel};
use crate::tui::model::{Conn, Model, Screen, Tab, palette_matches};
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
            Tab::Shell => shell::draw(f, model, rows[1]),
        },
    }

    draw_ticker(f, model, rows[2]);
    draw_footer(f, model, rows[3]);

    // Focus window (deploy) overlays everything.
    if let Some(fc) = &model.focus {
        focus::draw(f, model, fc);
    }
    if let Some(plan) = &model.plan {
        draw_plan(f, plan);
    }
    if let Some(wiz) = &model.wizard {
        draw_wizard(f, model, wiz);
    }
    if let Some(c) = &model.confirm {
        draw_confirm(f, c);
    }
    if let Some(q) = &model.yes_no {
        draw_yes_no(f, q);
    }
    if model.help_open {
        draw_help(f, model.tab);
    }
    if model.palette_open {
        draw_palette(f, model);
    }
}

/// The typed confirmation for a restore. Small, red-bordered, and it says
/// what the operation will overwrite rather than asking "are you sure".
fn draw_confirm(f: &mut Frame, c: &crate::tui::model::Confirm) {
    let area = f.area();
    let w = 64u16.min(area.width.saturating_sub(4));
    let h = 9u16.min(area.height.saturating_sub(2));
    let rect = Rect {
        x: (area.width.saturating_sub(w)) / 2,
        y: area.height / 3,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(THEME.red))
        .title(Line::from(Span::styled(
            format!(" >> CONFIRM {} << ", c.op.title()),
            Style::new().fg(THEME.red).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(Span::styled(c.prompt.clone(), Style::new().fg(THEME.text))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  > ", THEME.muted_style()),
            Span::styled(
                c.typed.clone(),
                Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", THEME.muted_style()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  ENTER confirm · ESC cancel",
            THEME.muted_style(),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}

/// fix-102: a one-key question. It states what `y` sets in motion; every
/// other key cancels, so a stray keystroke does nothing.
fn draw_yes_no(f: &mut Frame, q: &crate::tui::model::YesNo) {
    let area = f.area();
    let w = 76u16.min(area.width.saturating_sub(4));
    let h = 8u16.min(area.height.saturating_sub(2));
    let rect = Rect {
        x: (area.width.saturating_sub(w)) / 2,
        y: area.height / 3,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(THEME.yellow))
        .title(Line::from(Span::styled(
            format!(" >> {} << ", q.title),
            Style::new().fg(THEME.yellow).add_modifier(Modifier::BOLD),
        )))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(Span::styled(q.prompt.clone(), Style::new().fg(THEME.text))),
        Line::from(""),
        Line::from(Span::styled(
            "  y yes · any other key: no, nothing is done",
            THEME.muted_style(),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}

fn draw_plan(f: &mut Frame, plan: &crate::tui::model::Plan) {
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
    // fix-106: titles are read, so they never scramble.
    let title = format!("CHANGE_PLAN :: {}", plan.stack);
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
        WizStep::Storage => 4,
        WizStep::Review => 5,
    };
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_modal())
        .title(Line::from(Span::styled(
            format!(" >> STACK_FORGE :: STEP {}/5 << ", step_no),
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
    let crumbs = ["PRESET", "NAME", "RESOURCES", "STORAGE", "REVIEW"];
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
        WizStep::Storage => {
            // One row per /appdata directory this stack will get. The
            // question is deliberately about the APP rather than the path:
            // "keeps files of its own" is something Kenny knows about a
            // service, where "no_data: true" is something only the manifest
            // knows.
            let mut lines: Vec<Line> = vec![
                Line::from(Span::styled(
                    "  Which of these keeps files of its own?",
                    Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "  An app that keeps nothing gets no backup repository, so an",
                    THEME.muted_style(),
                )),
                Line::from(Span::styled(
                    "  empty directory is the design instead of a stopped backup.",
                    THEME.muted_style(),
                )),
                Line::from(""),
            ];
            for (i, path) in wiz.storage_paths.iter().enumerate() {
                let sel = i == wiz.storage_idx;
                let hollow = wiz.storage_no_data.get(i).copied().unwrap_or(false);
                let style = if sel {
                    Style::new()
                        .fg(THEME.cyan)
                        .bg(fx::pulse_bg(model.tick, model.fx))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(THEME.text)
                };
                lines.push(Line::from(vec![
                    Span::styled(if sel { "▶ " } else { "  " }, style),
                    Span::styled(
                        if hollow {
                            "[ keeps nothing ] "
                        } else {
                            "[ keeps files   ] "
                        },
                        if hollow { THEME.warn() } else { THEME.ok() },
                    ),
                    Span::styled(path.clone(), style),
                ]));
            }
            f.render_widget(Paragraph::new(lines), rows[1]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " ↑↓ choose · SPACE toggle · ENTER continue · ESC back ",
                    THEME.muted_style(),
                ))),
                rows[2],
            );
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
                    Span::styled(format!("  {:<9} ", k), THEME.muted_style()),
                    Span::styled(v, Style::new().fg(THEME.text)),
                ])
            };
            let lines = vec![
                Line::from(vec![
                    Span::styled("  name      ", THEME.muted_style()),
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
                kv("apps", app.to_string()),
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
    // The storage step draws its own hint line; the generic one drawn on
    // top of it left `[UP/DOWN] selectback` behind (finding 5).
    if !matches!(wiz.step, WizStep::Storage) {
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
}

fn draw_tab_bar(f: &mut Frame, model: &Model, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .map(|t| {
            let label = t.title();
            if *t == model.tab {
                Line::from(Span::styled(format!(" {} ", label), THEME.title_active()))
            } else {
                Line::from(Span::styled(format!(" {} ", label), THEME.title_inactive()))
            }
        })
        .collect();

    // fix-106 (see REGISTER.md): text that carries meaning stands still.
    let title = "HOMELAB :: CONTROL_DECK".to_string();

    let (dot, conn_txt, conn_style) = match model.conn {
        Conn::Up => ("● ", "HOST_LINK", THEME.ok()),
        Conn::Connecting => ("◍ ", "LINKING", Style::new().fg(THEME.yellow)),
        Conn::Down => ("○ ", "LINK_DOWN", THEME.err()),
    };

    // fix-105 (older-client-no-warning, 2026-09-27): which client talks to
    // which host, in yellow when this client is the older one (it may then
    // read, not change).
    let client = env!("CARGO_PKG_VERSION");
    let host = if model.host_version.is_empty() {
        "?".to_string()
    } else {
        model.host_version.clone()
    };
    let versions_style = if crate::version::older(client, &model.host_version) {
        Style::new().fg(THEME.yellow)
    } else {
        THEME.muted_style()
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
                Span::styled(
                    format!("client v{} · host v{} ", client, host),
                    versions_style,
                ),
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
    if let Some(tag) = model.host_update_available() {
        attn.push(format!(
            "⬆ HOST UPDATE {} available — CTRL+K, \"host update\" (signature and checksum \
             verified, auto-rollback armed)",
            tag
        ));
    }
    if let Some(fleet) = &model.fleet {
        let drifted: Vec<&str> = fleet
            .stacks
            .iter()
            .filter(|s| s.drift)
            .map(|s| s.name.as_str())
            .collect();
        if !drifted.is_empty() {
            // fix-107: CHANGED, not UPD, which read as SHIFT+U (update
            // the images) — this is files that differ from what was applied.
            attn.push(format!("⚠ CHANGED, not deployed: {}", drifted.join(",")));
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

    // fix-68 (four-answers-to-is-anything-wrong, 2026-09-27): the verdict is
    // the day's list, the one `homelab today` prints — doctor, check,
    // incidents and manual checks — and never "the containers run". The old
    // "ALL SYSTEMS NOMINAL" read none of those, and said it in the same
    // minute `homelab check` listed a broken item.
    let verdict = match &model.today {
        Some(t) if t.needs_you() => {
            attn.insert(
                0,
                format!(
                    "⚠ {} — the TODAY panel on the dashboard lists them",
                    t.verdict()
                ),
            );
            None
        }
        Some(t) => Some(format!("● {}", t.verdict().to_uppercase())),
        None if model.today_pending => Some("● checking what needs you…".to_string()),
        None => Some("● not yet checked what needs you — press r".to_string()),
    };

    // If anything needs attention, show that (yellow); else calm (faint).
    let (segs, color) = if !attn.is_empty() {
        (attn, THEME.yellow)
    } else {
        let mut c = vec![verdict.unwrap_or_default()];
        c.extend(calm);
        (c, THEME.faint)
    };
    // fix-106 (see REGISTER.md): the alerts stand still; what does not fit
    // is counted, and the TODAY panel lists everything.
    let text = static_line(&segs, area.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::new().fg(color).bg(THEME.bg),
        ))),
        area,
    );
}

/// fix-106: as many whole segments as fit in `width`, then how many more.
fn static_line(segs: &[String], width: usize) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        let sep = if out.is_empty() { "" } else { "  ::  " };
        let rest = segs.len() - i - 1;
        let more = if rest > 0 {
            format!("  ::  +{} more", rest)
        } else {
            String::new()
        };
        let need = out.chars().count() + sep.chars().count() + seg.chars().count();
        if need + more.chars().count() > width && !out.is_empty() {
            out.push_str(&format!("  ::  +{} more", segs.len() - i));
            return out.chars().take(width).collect();
        }
        out.push_str(sep);
        out.push_str(seg);
    }
    out.chars().take(width).collect()
}

fn draw_footer(f: &mut Frame, model: &Model, area: Rect) {
    // AZERTY: modifier names spelled out, digit-row hints shown as "1-6".
    // A letter is shown exactly as it is pressed: lowercase for a plain key,
    // SHIFT+ for a capital. `[R] refresh` once sat beside SHIFT+R = restore,
    // and the ticker's "press U" beside SHIFT+U = update the selected stack
    // (test-plan part A, 2026-09-26, finding 2).
    // fix-107: from the one key table the key map and the palette use.
    let keys = crate::tui::keys::footer(model.tab);
    // The status sits on the right and the hints take what is left, whole
    // hints only. Both used to be drawn across the full width, so a status
    // landed on top of the last hints: `[Q]link established` (finding 1).
    let status = format!("{} ", model.status_line);
    let status_w = if model.status_line.is_empty() {
        0
    } else {
        status.chars().count() as u16
    };
    let room = area.width.saturating_sub(status_w + 1) as usize;
    let mut spans: Vec<Span> = Vec::new();
    let mut used = 0usize;
    for (k, d) in keys {
        let key = format!("[{}]", k);
        let desc = format!(" {}  ", d);
        let w = key.chars().count() + desc.chars().count();
        if used + w > room {
            break;
        }
        used += w;
        spans.push(Span::styled(key, THEME.hint()));
        spans.push(Span::styled(desc, THEME.muted_style()));
    }
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(status_w)]).split(area);
    let (hints, right) = (cols[0], cols[1]);
    f.render_widget(Paragraph::new(Line::from(spans)), hints);
    f.render_widget(
        Paragraph::new(
            Line::from(Span::styled(status, Style::new().fg(THEME.blue))).right_aligned(),
        ),
        right,
    );
}

/// fix-107 (see REGISTER.md): the key map of the tab in front of the
/// operator, from the one key table.
fn draw_help(f: &mut Frame, tab: Tab) {
    let area = f.area();
    let w = 76u16.min(area.width - 4);
    let h = area.height.saturating_sub(2);
    let rect = Rect {
        x: (area.width - w) / 2,
        y: area.height / 8,
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
    let lines: Vec<Line> = crate::tui::keys::KEYMAP
        .iter()
        .filter(|b| b.shown_on(tab) && !b.key.is_empty())
        .map(|b| {
            Line::from(vec![
                Span::styled(format!("  {:<12}", b.key), THEME.hint()),
                Span::styled(b.what, Style::new().fg(THEME.text)),
            ])
        })
        .chain(std::iter::once(Line::from(Span::styled(
            "  CTRL+K lists every action by name; other tabs have their own keys",
            THEME.muted_style(),
        ))))
        .chain(std::iter::once(Line::from(Span::styled(
            "  host settings (nightly hour, retention, webhook): the admin \
             dashboard, or `homelab host apply`",
            THEME.muted_style(),
        ))))
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
    let actions = crate::tui::keys::palette();
    // fix-107: "selected stack" says which one.
    let selected = model
        .fleet
        .as_ref()
        .and_then(|f| f.stacks.get(model.selected_stack))
        .map(|s| s.name.clone());
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
                Span::styled(
                    match &selected {
                        Some(name) => actions[ai].label.replace("selected stack", name),
                        None => actions[ai].label.clone(),
                    },
                    style,
                ),
            ]))
        })
        .collect();
    f.render_widget(List::new(items), rows[2]);
}

/// Shared panel title helper.
pub fn panel_title(text: &str, id: u64, model: &Model) -> Line<'static> {
    // fix-106: never scrambled; `id` and the effect level no longer matter.
    let _ = (id, model.fx);
    let t = text.to_string();
    Line::from(vec![
        Span::styled(" [ ", Style::new().fg(THEME.faint)),
        Span::styled(t, THEME.title_active()),
        Span::styled(" ] ", Style::new().fg(THEME.faint)),
    ])
}
