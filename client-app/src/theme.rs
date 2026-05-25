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
            background: Color::Rgb(20, 22, 34),
            border: Color::Rgb(60, 60, 80),
            text: Color::White,
            warning: Color::Red,
        }
    }

    pub fn tab_style(&self, active: bool) -> Style {
        if active {
            Style::default()
                .fg(self.accent_cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.text)
        }
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn warning_style(&self) -> Style {
        Style::default()
            .fg(self.warning)
            .add_modifier(Modifier::BOLD)
    }
}
