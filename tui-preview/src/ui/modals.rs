//! Modals: help, new-stack wizard, deploy diff preview, deploy progress,
//! and the typed-confirmation delete.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, Gauge, Paragraph};

use crate::app::{App, Modal, WizardStep, PRESETS};
use crate::fx;
use crate::sim::StepState;
use crate::theme::THEME;

fn modal_rect(f: &Frame, w: u16, h: u16) -> Rect {
    let area = f.area();
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(4));
    Rect {
        x: (area.width - w) / 2,
        y: (area.height.saturating_sub(h)) / 3,
        width: w,
        height: h,
    }
}

fn modal_block(title: &str, danger: bool, app: &App) -> Block<'static> {
    let t = fx::glitch(title, 0x30DA1, app.tick, app.fx).unwrap_or_else(|| title.to_string());
    Block::bordered()
        .border_type(BorderType::Double)
        .border_style(if danger { THEME.border_danger() } else { THEME.border_modal() })
        .title(Line::from(vec![
            Span::styled(" >> ", Style::new().fg(THEME.faint)),
            Span::styled(
                t,
                if danger {
                    THEME.err().add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD)
                },
            ),
            Span::styled(" << ", Style::new().fg(THEME.faint)),
        ]))
        .style(Style::new().bg(THEME.elevated).fg(THEME.text))
}

pub fn draw(f: &mut Frame, app: &mut App) {
    // Note: reads app.modal immutably; all data needed is cloned up front.
    match &app.modal {
        Modal::Help => draw_help(f, app),
        Modal::Wizard(_) => draw_wizard(f, app),
        Modal::Diff { stack_idx, scroll } => {
            let (idx, sc) = (*stack_idx, *scroll);
            draw_diff(f, app, idx, sc);
        }
        Modal::Deploy => draw_deploy(f, app),
        Modal::ConfirmDelete { stack_idx, input } => {
            let (idx, text) = (*stack_idx, input.clone());
            draw_delete(f, app, idx, &text);
        }
        Modal::None => {}
    }
}

fn draw_help(f: &mut Frame, app: &App) {
    let rect = modal_rect(f, 62, 18);
    f.render_widget(Clear, rect);
    let block = modal_block("KEYMAP :: CONTROL_DECK", false, app);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows: Vec<(&str, &str)> = vec![
        ("1-4 / Tab", "switch tabs"),
        ("j / k", "move selection"),
        ("n", "new stack (preset wizard)"),
        ("D", "deploy → diff preview → live progress"),
        ("a / x", "activate / deactivate stack"),
        ("b", "restic backup for selected stack"),
        ("d", "delete stack (typed confirmation)"),
        ("space / f", "logs: follow / level filter"),
        ("Ctrl+K", "command palette"),
        ("F2", "cycle FX intensity (off/subtle/full)"),
        ("?", "this help"),
        ("q", "quit"),
    ];
    let lines: Vec<Line> = rows
        .into_iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {:<12}", k), THEME.hint()),
                Span::styled(v.to_string(), Style::new().fg(THEME.text)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_wizard(f: &mut Frame, app: &mut App) {
    let Modal::Wizard(w) = &app.modal else { return };
    let rect = modal_rect(f, 66, 20);
    f.render_widget(Clear, rect);

    let step_no = match w.step {
        WizardStep::Preset => 1,
        WizardStep::Name => 2,
        WizardStep::Resources => 3,
        WizardStep::Review => 4,
    };
    let block = modal_block(&format!("STACK_FORGE :: STEP {}/4", step_no), false, app);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(4), Constraint::Length(1)])
        .split(inner);

    // Step breadcrumb.
    let crumbs = ["PRESET", "NAME", "RESOURCES", "REVIEW"];
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, c) in crumbs.iter().enumerate() {
        let active = i + 1 == step_no;
        spans.push(Span::styled(
            format!(" {} ", c),
            if active {
                Style::new().fg(THEME.bg).bg(THEME.cyan).add_modifier(Modifier::BOLD)
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

    match w.step {
        WizardStep::Preset => {
            let lines: Vec<Line> = PRESETS
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let selected = i == w.preset_idx;
                    let marker = if selected { "▶ " } else { "  " };
                    let style = if selected {
                        Style::new()
                            .fg(THEME.cyan)
                            .bg(fx::pulse_bg(app.tick, app.fx))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(THEME.text)
                    };
                    Line::from(vec![
                        Span::styled(marker.to_string(), style),
                        Span::styled(format!("{:<14}", p.name), style),
                        Span::styled(p.desc.to_string(), THEME.muted_style()),
                    ])
                })
                .collect();
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizardStep::Name => {
            let cursor = if (app.tick / 15) % 2 == 0 { "█" } else { " " };
            let lines = vec![
                Line::from(Span::styled("stack name (lowercase, single word):", THEME.muted_style())),
                Line::default(),
                Line::from(vec![
                    Span::styled("  λ ", Style::new().fg(THEME.cyan)),
                    Span::styled(
                        w.name.clone(),
                        Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(cursor, Style::new().fg(THEME.cyan)),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("  hostname will be ", THEME.muted_style()),
                    Span::styled(
                        format!("{}-app-{}", app.world.next_free_vmid(), w.name),
                        Style::new().fg(THEME.green),
                    ),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizardStep::Resources => {
            let lines = vec![
                Line::from(vec![
                    Span::styled("  RAM   ", THEME.muted_style()),
                    Span::styled(format!("{:>5} MB", w.ram), Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("   [+/-] adjust", THEME.hint()),
                ]),
                Line::from(vec![
                    Span::styled("  CORES ", THEME.muted_style()),
                    Span::styled(format!("{:>5}", w.cores), Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("      [c] cycle", THEME.hint()),
                ]),
                Line::from(vec![
                    Span::styled("  DISK  ", THEME.muted_style()),
                    Span::styled(format!("{:>4} GB", w.disk), Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("     [s] cycle", THEME.hint()),
                ]),
                Line::default(),
                Line::from(Span::styled(
                    "  core apps promtail + watchtower are added automatically",
                    Style::new().fg(THEME.faint),
                )),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
        WizardStep::Review => {
            let preset = &PRESETS[w.preset_idx];
            let vmid = app.world.next_free_vmid();
            let apps: Vec<String> = preset.apps.iter().map(|(n, _)| n.to_string()).collect();
            let lines = vec![
                Line::from(vec![
                    Span::styled("  stack     ", THEME.muted_style()),
                    Span::styled(w.name.clone(), Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("  hostname  ", THEME.muted_style()),
                    Span::styled(format!("{}-app-{}", vmid, w.name), Style::new().fg(THEME.text)),
                ]),
                Line::from(vec![
                    Span::styled("  network   ", THEME.muted_style()),
                    Span::styled(
                        format!("10.10.10.{} · vlan 10 · MAC derived", vmid - 100),
                        Style::new().fg(THEME.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  resources ", THEME.muted_style()),
                    Span::styled(
                        format!("{} MB · {} cores · {} GB", w.ram, w.cores, w.disk),
                        Style::new().fg(THEME.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  apps      ", THEME.muted_style()),
                    Span::styled(
                        if apps.is_empty() { "(none yet)".into() } else { apps.join(", ") },
                        Style::new().fg(THEME.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  deploy    ", THEME.muted_style()),
                    Span::styled("enabled=false — nothing runs until you activate", THEME.warn()),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("  ↵ ", THEME.hint()),
                    Span::styled("scaffold stack (repo only, fully reversible)", THEME.ok()),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), rows[1]);
        }
    }

    let footer = Line::from(vec![
        Span::styled("[↵]", THEME.hint()),
        Span::styled(" next  ", THEME.muted_style()),
        Span::styled("[esc]", THEME.hint()),
        Span::styled(" back/cancel", THEME.muted_style()),
    ]);
    f.render_widget(Paragraph::new(footer), rows[2]);
}

fn draw_diff(f: &mut Frame, app: &App, stack_idx: usize, scroll: u16) {
    let Some(stack) = app.world.stacks.get(stack_idx) else { return };
    let rect = modal_rect(f, 72, 20);
    f.render_widget(Clear, rect);
    let block = modal_block(&format!("CHANGE_PLAN :: {}", stack.hostname()), false, app);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "dry-run against HOST state — nothing has been touched yet",
            Style::new().fg(THEME.faint),
        )),
        Line::default(),
    ];
    let diff: Vec<(char, String)> = vec![
        (' ', format!("stacks/{}/lxc-compose.yml", stack.name)),
        (' ', "  resources:".into()),
        ('-', format!("    memory_mb: {}", stack.ram_limit_mb)),
        ('+', format!("    memory_mb: {}", stack.ram_limit_mb)),
        (' ', format!("stacks/{}/{}/docker-compose.yml", stack.name, stack.apps.first().map(|a| a.name).unwrap_or("app"))),
        ('-', "    image: pinned@old-digest".into()),
        ('+', "    image: pinned@new-digest".into()),
        (' ', "".into()),
        (' ', "plan:".into()),
        ('~', format!("  UPDATE   {} (config changed, in-place)", stack.hostname())),
        (' ', "  SKIP     everything else (no drift)".into()),
        (' ', "".into()),
        (' ', "safety:".into()),
        (' ', "  ✓ hostname guard will verify before any change".into()),
        (' ', "  ✓ fail-closed: errors set deploy.enabled=false".into()),
        (' ', "  ✓ no-touch list: 100,101,102,103,201-203 invisible".into()),
    ];
    for (sign, text) in diff.into_iter().skip(scroll as usize) {
        let (prefix, style) = match sign {
            '+' => ("+ ", THEME.ok()),
            '-' => ("- ", THEME.err()),
            '~' => ("~ ", THEME.warn()),
            _ => ("  ", Style::new().fg(THEME.text)),
        };
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(text, style),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("[↵]", THEME.hint()),
        Span::styled(" execute deploy   ", THEME.muted_style()),
        Span::styled("[esc]", THEME.hint()),
        Span::styled(" abort   ", THEME.muted_style()),
        Span::styled("[j/k]", THEME.hint()),
        Span::styled(" scroll", THEME.muted_style()),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_deploy(f: &mut Frame, app: &App) {
    let Some(d) = &app.world.deploy else { return };
    let stack = &app.world.stacks[d.stack_idx];
    let rect = modal_rect(f, 70, 19);
    f.render_widget(Clear, rect);
    let title = if d.finished {
        format!("DEPLOY :: {} :: COMPLETE", stack.hostname())
    } else {
        format!("DEPLOY :: {} :: LIVE", stack.hostname())
    };
    let block = modal_block(&title, false, app);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows = Layout::vertical([
        Constraint::Length(d.steps.len() as u16),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let step_lines: Vec<Line> = d
        .steps
        .iter()
        .map(|(name, state)| {
            let (icon, style) = match state {
                StepState::Pending => ("○".to_string(), THEME.muted_style()),
                StepState::Running => (fx::spinner(app.tick).to_string(), Style::new().fg(THEME.cyan).add_modifier(Modifier::BOLD)),
                StepState::Done => ("✓".to_string(), THEME.ok()),
            };
            Line::from(vec![
                Span::styled(format!("  {} ", icon), style),
                Span::styled(
                    name.to_string(),
                    if *state == StepState::Running {
                        Style::new().fg(THEME.text).add_modifier(Modifier::BOLD)
                    } else if *state == StepState::Done {
                        Style::new().fg(THEME.text)
                    } else {
                        THEME.muted_style()
                    },
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(step_lines), rows[0]);

    let done = d.steps.iter().filter(|(_, s)| *s == StepState::Done).count();
    let gauge = Gauge::default()
        .ratio(done as f64 / d.steps.len() as f64)
        .gauge_style(Style::new().fg(if d.finished { THEME.green } else { THEME.cyan }).bg(THEME.panel))
        .label(Span::styled(
            format!("{}/{}", done, d.steps.len()),
            Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(gauge, rows[1]);

    let visible = rows[2].height as usize;
    let start = d.log.len().saturating_sub(visible);
    let log_lines: Vec<Line> = d.log[start..]
        .iter()
        .map(|l| Line::from(Span::styled(format!("  {}", l), Style::new().fg(THEME.faint))))
        .collect();
    f.render_widget(Paragraph::new(log_lines), rows[2]);

    let footer = if d.finished {
        Line::from(vec![
            Span::styled("● ALL GATES PASSED ", THEME.ok().add_modifier(Modifier::BOLD)),
            Span::styled("— [↵] close", THEME.muted_style()),
        ])
    } else {
        Line::from(Span::styled("  streaming from HOST over TLS…", Style::new().fg(THEME.faint)))
    };
    f.render_widget(Paragraph::new(footer), rows[3]);
}

fn draw_delete(f: &mut Frame, app: &App, stack_idx: usize, input: &str) {
    let Some(stack) = app.world.stacks.get(stack_idx) else { return };
    let rect = modal_rect(f, 60, 11);
    f.render_widget(Clear, rect);
    let block = modal_block("DANGER :: REMOVE_STACK", true, app);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let ok = input == stack.name;
    let cursor = if (app.tick / 15) % 2 == 0 { "█" } else { " " };
    let lines = vec![
        Line::from(vec![
            Span::styled("This removes ", Style::new().fg(THEME.text)),
            Span::styled(stack.name.clone(), THEME.err().add_modifier(Modifier::BOLD)),
            Span::styled(" from the repo.", Style::new().fg(THEME.text)),
        ]),
        Line::from(Span::styled(
            "The LXC itself is NOT destroyed — that is a separate, guarded action.",
            THEME.warn(),
        )),
        Line::default(),
        Line::from(vec![
            Span::styled("type the stack name to confirm: ", THEME.muted_style()),
        ]),
        Line::from(vec![
            Span::styled("  λ ", Style::new().fg(THEME.red)),
            Span::styled(
                input.to_string(),
                if ok { THEME.ok().add_modifier(Modifier::BOLD) } else { Style::new().fg(THEME.text) },
            ),
            Span::styled(cursor, Style::new().fg(THEME.red)),
        ]),
        Line::default(),
        Line::from(vec![
            Span::styled("[↵]", if ok { THEME.hint() } else { THEME.muted_style() }),
            Span::styled(if ok { " confirmed — execute  " } else { " (name mismatch)  " }, THEME.muted_style()),
            Span::styled("[esc]", THEME.hint()),
            Span::styled(" abort", THEME.muted_style()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}
