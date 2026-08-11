//! Dashboard: HOST_MESH status, LXC fleet table with live sparklines, and the
//! right-hand column of ops panels (ROLLBACK_GUARD, SECRETS_VAULT, GITOPS_ENGINE).

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table};

use crate::app::App;
use crate::fx;
use crate::sim::{AppState, StackStatus};
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Min(58), Constraint::Length(34)]).split(area);
    let left = Layout::vertical([Constraint::Length(6), Constraint::Min(8)]).split(cols[0]);

    draw_host(f, app, left[0]);
    draw_fleet(f, app, left[1]);
    draw_side(f, app, cols[1]);
}

fn panel_title(text: &str, id: u64, app: &App) -> Line<'static> {
    let t = fx::glitch(text, id, app.tick, app.fx).unwrap_or_else(|| text.to_string());
    Line::from(vec![
        Span::styled(" [ ", Style::new().fg(THEME.faint)),
        Span::styled(t, THEME.title_active()),
        Span::styled(" ] ", Style::new().fg(THEME.faint)),
    ])
}

fn draw_host(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.world;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_active())
        .title(panel_title(
            &format!("HOST_MESH :: {}", w.host.name),
            0x4057,
            app,
        ))
        .title(
            Line::from(vec![
                Span::styled("● ", THEME.ok()),
                Span::styled("[ONLINE] ", THEME.ok()),
                Span::styled(format!("up {} ", w.host.uptime), THEME.muted_style()),
            ])
            .right_aligned(),
        )
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let cpu = w.host.cpu.last();
    let ram = w.host.ram.last();
    let spark_cpu = fx::braille_spark(&w.host.cpu.slice(), 24);
    let spark_ram = fx::braille_spark(&w.host.ram.slice(), 24);

    let line1 = Line::from(vec![
        Span::styled("CPU ", THEME.muted_style()),
        Span::styled(
            format!("{:>3}% ", cpu),
            Style::new()
                .fg(fx::load_color(cpu))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(spark_cpu, Style::new().fg(THEME.cyan)),
        Span::styled(
            format!("   TEMP {:>2.0}°C", w.host.temp),
            THEME.muted_style(),
        ),
    ]);
    let line2 = Line::from(vec![
        Span::styled("RAM ", THEME.muted_style()),
        Span::styled(
            format!("{:>3}% ", ram),
            Style::new()
                .fg(fx::load_color(ram))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(spark_ram, Style::new().fg(THEME.magenta)),
        Span::styled(format!("   DISK {}%", w.host.disk_pct), THEME.muted_style()),
    ]);
    let disk_bar: String = {
        let filled = (w.host.disk_pct as usize * 30) / 100;
        format!("{}{}", "█".repeat(filled), "░".repeat(30 - filled))
    };
    let line3 = Line::from(vec![
        Span::styled("SSD ", THEME.muted_style()),
        Span::styled(disk_bar, Style::new().fg(THEME.blue)),
        Span::styled(
            format!("  TLS {}", w.host.tls_fingerprint),
            Style::new().fg(THEME.green),
        ),
    ]);
    let line4 = Line::from(vec![
        Span::styled("LNK ", THEME.muted_style()),
        Span::styled(
            format!("ws://control ↔ host  {:.1}ms  ", w.link_latency_ms),
            Style::new().fg(THEME.text),
        ),
        Span::styled("keepalive ", THEME.muted_style()),
        Span::styled(
            fx::spinner(app.tick).to_string(),
            Style::new().fg(THEME.cyan),
        ),
    ]);

    f.render_widget(Paragraph::new(line1), rows[0]);
    f.render_widget(Paragraph::new(line2), rows[1]);
    f.render_widget(Paragraph::new(line3), rows[2]);
    f.render_widget(Paragraph::new(line4), rows[3]);
}

fn draw_fleet(f: &mut Frame, app: &mut App, area: Rect) {
    let w = &app.world;
    let n_online = w
        .stacks
        .iter()
        .filter(|s| s.status == StackStatus::Online)
        .count();
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(panel_title(
            &format!("LXC_MESH :: {}/{} NODES", n_online, w.stacks.len()),
            0x1E51,
            app,
        ))
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Row::new(vec![
        Cell::from("NODE"),
        Cell::from("STATUS"),
        Cell::from("CPU"),
        Cell::from(""),
        Cell::from("RAM"),
        Cell::from(""),
        Cell::from("APPS"),
        Cell::from("FLAGS"),
    ])
    .style(Style::new().fg(THEME.muted).add_modifier(Modifier::BOLD));

    let scan = fx::scanline(w.stacks.len() as u16, app.tick, 0x51, app.fx);

    let rows: Vec<Row> = w
        .stacks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let color = THEME.stack_color(&s.name);
            let (dot, status_style) = match s.status {
                StackStatus::Online => ("●", THEME.ok()),
                StackStatus::Syncing => ("◍", Style::new().fg(THEME.cyan)),
                StackStatus::Degraded => ("◐", THEME.warn()),
                StackStatus::Offline => ("○", THEME.muted_style()),
            };
            let running = s
                .apps
                .iter()
                .filter(|a| a.state == AppState::Running)
                .count();
            let cpu = s.cpu.last();
            let ram = s.ram.last();
            let mut flags = String::new();
            if s.drift {
                flags.push_str("[UPD] ");
            }
            if !s.sealed {
                flags.push_str("[NOENV] ");
            }
            if !s.enabled {
                flags.push_str("[OFF]");
            }
            let mut row = Row::new(vec![
                Cell::from(Line::from(vec![
                    Span::styled("▎", Style::new().fg(color)),
                    Span::styled(
                        s.hostname(),
                        Style::new().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ])),
                Cell::from(Line::from(vec![
                    Span::styled(format!("{} ", dot), status_style),
                    Span::styled(s.status.label(), status_style),
                ])),
                Cell::from(Span::styled(
                    format!("{:>3}%", cpu),
                    Style::new().fg(fx::load_color(cpu)),
                )),
                Cell::from(Span::styled(
                    fx::braille_spark(&s.cpu.slice(), 10),
                    Style::new().fg(THEME.cyan),
                )),
                Cell::from(Span::styled(
                    format!("{:>3}%", ram),
                    Style::new().fg(fx::load_color(ram)),
                )),
                Cell::from(Span::styled(
                    fx::braille_spark(&s.ram.slice(), 10),
                    Style::new().fg(THEME.magenta),
                )),
                Cell::from(Span::styled(
                    format!("{}/{}", running, s.apps.len()),
                    if running == s.apps.len() {
                        THEME.ok()
                    } else {
                        THEME.warn()
                    },
                )),
                Cell::from(Span::styled(flags, THEME.warn())),
            ]);
            if Some(i as u16) == scan {
                row = row.style(Style::new().bg(Color::Rgb(0x0D, 0x22, 0x26)));
            }
            row
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(11),
            Constraint::Length(4),
            Constraint::Length(11),
            Constraint::Length(4),
            Constraint::Length(11),
            Constraint::Length(5),
            Constraint::Min(6),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(fx::pulse_bg(app.tick, app.fx))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Span::styled("▶ ", Style::new().fg(THEME.cyan)));

    f.render_stateful_widget(table, inner, &mut app.stack_table);
}

fn draw_side(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Min(6),
    ])
    .split(area);

    // ROLLBACK_GUARD
    let sel = app.selected_stack();
    let stack = &app.world.stacks[sel];
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_modal())
        .title(panel_title("ROLLBACK_GUARD", 0x0511, app))
        .style(THEME.panel_style());
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);
    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("Auto-rollback: ", THEME.muted_style()),
        Span::styled("● ARMED", THEME.ok().add_modifier(Modifier::BOLD)),
        Span::styled(" (10s)", THEME.muted_style()),
    ])];
    for a in stack
        .apps
        .iter()
        .take((inner.height as usize).saturating_sub(1))
    {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<12}", a.name), Style::new().fg(THEME.text)),
            Span::styled(format!("sha:{}", a.digest), Style::new().fg(THEME.faint)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);

    // SECRETS_VAULT
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_modal())
        .title(panel_title("SECRETS_VAULT", 0x5EC7, app))
        .style(THEME.panel_style());
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);
    let mut lines: Vec<Line> = Vec::new();
    for s in app.world.stacks.iter().take(inner.height as usize - 1) {
        let (icon, style) = if s.sealed {
            ("● sealed", THEME.ok())
        } else {
            ("○ missing", THEME.err())
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<12}", s.name),
                Style::new().fg(THEME.stack_color(&s.name)),
            ),
            Span::styled(icon, style),
            Span::styled("  0600", THEME.muted_style()),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "values never leave HOST",
        Style::new().fg(THEME.faint),
    )));
    f.render_widget(Paragraph::new(lines), inner);

    // GITOPS_ENGINE
    let g = &app.world.git;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_modal())
        .title(panel_title("GITOPS_ENGINE", 0x617, app))
        .style(THEME.panel_style());
    let inner = block.inner(rows[2]);
    f.render_widget(block, rows[2]);
    let drifted = app.world.stacks.iter().filter(|s| s.drift).count();
    let lines = vec![
        Line::from(vec![
            Span::styled("branch ", THEME.muted_style()),
            Span::styled(g.branch.clone(), Style::new().fg(THEME.cyan)),
            Span::styled(format!(" @ {}", g.commit), Style::new().fg(THEME.text)),
        ]),
        Line::from(vec![
            Span::styled("mirror ", THEME.muted_style()),
            if g.mirror_ok {
                Span::styled("● github ok", THEME.ok())
            } else {
                Span::styled("○ github behind", THEME.warn())
            },
        ]),
        Line::from(vec![
            Span::styled("drift  ", THEME.muted_style()),
            if drifted == 0 {
                Span::styled("none — intent == runtime", THEME.ok())
            } else {
                Span::styled(format!("{} stack(s) [UPD]", drifted), THEME.warn())
            },
        ]),
        Line::from(vec![
            Span::styled("commits today ", THEME.muted_style()),
            Span::styled(format!("{}", g.commits_today), Style::new().fg(THEME.text)),
        ]),
        Line::from(Span::styled(
            format!("» {}", g.last_msg),
            Style::new().fg(THEME.faint),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}
