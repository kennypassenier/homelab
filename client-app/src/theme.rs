//! Centralized theme and color palette for the Homelab Client TUI.
use ratatui::style::{Color, Modifier, Style};

/// Defines the cyberpunk color palette and style helpers.
pub struct Theme {
    pub accent_cyan: Color,
    pub accent_magenta: Color,
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub warning: Color,
}

impl Theme {
    pub fn cyberpunk() -> Self {
        Self {
            accent_cyan: Color::Cyan,
            accent_magenta: Color::Magenta,
            background: Color::Rgb(10, 10, 16),
            border: Color::Rgb(45, 55, 70),
            text: Color::White,
            warning: Color::Red,
        }
    }

    /// Dimmed border for inactive / secondary panels.
    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// Bright cyan border for the active / focused panel.
    pub fn active_border_style(&self) -> Style {
        Style::default()
            .fg(self.accent_cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// Magenta border — used for modals and elevated panels.
    pub fn modal_border_style(&self) -> Style {
        Style::default()
            .fg(self.accent_magenta)
            .add_modifier(Modifier::BOLD)
    }

    /// Active tab label: neon cyan + bold.
    pub fn tab_style(&self, active: bool) -> Style {
        if active {
            Style::default()
                .fg(self.accent_cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(100, 110, 130))
        }
    }

    /// Dimmed / powered-down text.
    pub fn dim_style(&self) -> Style {
        Style::default().fg(Color::Rgb(55, 60, 75))
    }

    /// Success / online state — neon green.
    pub fn success_style(&self) -> Style {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    }

    /// Warning style — red bold.
    pub fn warning_style(&self) -> Style {
        Style::default()
            .fg(self.warning)
            .add_modifier(Modifier::BOLD)
    }

    /// Sinusoidal pulse style for the selected/highlighted list item.
    ///
    /// `phase` comes from `App::pulse_phase` which advances 0.08 rad/tick at 30 FPS.
    /// Returns a style that oscillates between a dim teal and a bright cyan background.
    pub fn pulse_style(phase: f32) -> Style {
        let t = (phase.sin() * 0.5 + 0.5_f32); // 0.0 → 1.0
        let g = (t * 170.0) as u8;
        let b = (t * 210.0) as u8;
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(0, g, b))
            .add_modifier(Modifier::BOLD)
    }
}

