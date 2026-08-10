//! Backups tab: restic snapshot ledger, schedule policy, live backup progress.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, Gauge, Paragraph, Row, Table};

use crate::app::App;
use crate::fx;
use crate::theme::THEME;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Min(46), Constraint::Length(36)]).split(area);
    draw_snapshots(f, app, cols[0]);
    draw_policy(f, app, cols[1]);
}

fn draw_snapshots(f: &mut Frame, app: &mut App, area: Rect) {
    let title = fx::glitch("RESTIC_LEDGER", 0xBAC7, app.tick, app.fx)
        .unwrap_or_else(|| "RESTIC_LEDGER".into());
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(THEME.border_active())
        .title(Line::from(vec![
            Span::styled(" [ ", Style::new().fg(THEME.faint)),
            Span::styled(title, THEME.title_active()),
            Span::styled(" ] ", Style::new().fg(THEME.faint)),
        ]))
        .title(
            Line::from(Span::styled(
                "repo rclone:gdrive:homelab ",
                THEME.muted_style(),
            ))
            .right_aligned(),
        )
        .style(THEME.panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Row::new(vec!["ID", "STACK", "WHEN", "SIZE"])
        .style(Style::new().fg(THEME.muted).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .world
        .snapshots
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(Span::styled(s.id.clone(), Style::new().fg(THEME.faint))),
                Cell::from(Span::styled(
                    s.stack.clone(),
                    Style::new().fg(THEME.stack_color(&s.stack)).add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(s.time.clone(), Style::new().fg(THEME.text))),
                Cell::from(Span::styled(s.size.clone(), Style::new().fg(THEME.cyan))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(18),
            Constraint::Min(7),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(fx::pulse_bg(app.tick, app.fx))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(Span::styled("▶ ", Style::new().fg(THEME.cyan)));
    f.render_stateful_widget(table, inner, &mut app.snap_table);
}

fn draw_policy(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(8), Constraint::Length(6), Constraint::Min(4)])
        .split(area);

    // Schedule policy.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_modal())
        .title(Line::from(Span::styled(
            " [ POLICY :: SCHEDULE ] ",
            Style::new().fg(THEME.magenta).add_modifier(Modifier::BOLD),
        )))
        .style(THEME.panel_style());
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);
    let lines = vec![
        Line::from(vec![
            Span::styled("mode      ", THEME.muted_style()),
            Span::styled("interval (continuous-service)", Style::new().fg(THEME.text)),
        ]),
        Line::from(vec![
            Span::styled("interval  ", THEME.muted_style()),
            Span::styled("24h", Style::new().fg(THEME.cyan)),
            Span::styled("   next in ", THEME.muted_style()),
            Span::styled("6h 12m", Style::new().fg(THEME.text)),
        ]),
        Line::from(vec![
            Span::styled("retention ", THEME.muted_style()),
            Span::styled("7d / 4w / 3m", Style::new().fg(THEME.text)),
            Span::styled("  + prune", THEME.muted_style()),
        ]),
        Line::from(vec![
            Span::styled("quiesce   ", THEME.muted_style()),
            Span::styled("com.homelab.backup.pause=true", Style::new().fg(THEME.faint)),
        ]),
        Line::from(vec![
            Span::styled("notify    ", THEME.muted_style()),
            Span::styled("→ Home Assistant dispatcher", Style::new().fg(THEME.green)),
        ]),
        Line::from(vec![
            Span::styled("lock      ", THEME.muted_style()),
            Span::styled("single-cycle guard armed", THEME.ok()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);

    // Live backup progress.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if app.world.backup.is_some() {
            THEME.border_active()
        } else {
            THEME.border_inactive()
        })
        .title(Line::from(Span::styled(
            " [ CYCLE_MONITOR ] ",
            THEME.title_active(),
        )))
        .style(THEME.panel_style());
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);

    if let Some(b) = &app.world.backup {
        let stack = &app.world.stacks[b.stack_idx];
        let areas = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(fx::spinner(app.tick).to_string(), Style::new().fg(THEME.cyan)),
                Span::styled(
                    format!(" snapshotting {} ", stack.name),
                    Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("({:.0} MB read)", b.bytes_done), THEME.muted_style()),
            ])),
            areas[0],
        );
        let gauge = Gauge::default()
            .ratio(b.progress.min(1.0))
            .gauge_style(Style::new().fg(THEME.cyan).bg(THEME.elevated))
            .label(Span::styled(
                format!("{:>3.0}%", b.progress * 100.0),
                Style::new().fg(THEME.text).add_modifier(Modifier::BOLD),
            ));
        f.render_widget(gauge, areas[1]);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "pause labels honored · retention applies on success",
                Style::new().fg(THEME.faint),
            ))),
            areas[2],
        );
    } else {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("idle — no cycle running", THEME.muted_style())),
                Line::from(vec![
                    Span::styled("press ", THEME.muted_style()),
                    Span::styled("[b]", THEME.hint()),
                    Span::styled(" to snapshot the selected stack", THEME.muted_style()),
                ]),
            ]),
            inner,
        );
    }

    // Restore hint panel.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(THEME.border_inactive())
        .title(Line::from(Span::styled(
            " [ RESTORE ] ",
            THEME.title_inactive(),
        )))
        .style(THEME.panel_style());
    let inner = block.inner(rows[2]);
    f.render_widget(block, rows[2]);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "restore = validate → quiesce →",
                Style::new().fg(THEME.faint),
            )),
            Line::from(Span::styled(
                "rsync /appdata → resync apps",
                Style::new().fg(THEME.faint),
            )),
        ]),
        inner,
    );
}
