//! Cyberpunk color system. Every color in the UI comes from here — nothing is
//! hard-coded at call sites, so the whole look can be retuned in one place.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    // Surfaces
    pub bg: Color,       // deep canvas
    pub panel: Color,    // panel background
    pub elevated: Color, // modals / elevated surfaces
    pub dim: Color,      // dimmed background (flicker dark phase)

    // Accents
    pub cyan: Color,    // primary accent: active borders, titles, selection
    pub magenta: Color, // secondary accent: modals, glitch flashes
    pub green: Color,   // success / [ONLINE]
    pub yellow: Color,  // warnings / hints
    pub red: Color,     // errors / danger confirmations
    pub blue: Color,    // info accents

    // Text
    pub text: Color,  // primary text
    pub muted: Color, // secondary text
    pub faint: Color, // barely-there text (ticker, ghost rows)
}

pub const THEME: Theme = Theme {
    bg: Color::Rgb(0x0A, 0x0A, 0x10),
    panel: Color::Rgb(0x10, 0x12, 0x1C),
    elevated: Color::Rgb(0x16, 0x18, 0x24),
    dim: Color::Rgb(0x07, 0x07, 0x0B),

    cyan: Color::Rgb(0x00, 0xFF, 0xFF),
    magenta: Color::Rgb(0xFF, 0x00, 0xFF),
    green: Color::Rgb(0x39, 0xFF, 0x6E),
    yellow: Color::Rgb(0xE8, 0xF0, 0x22),
    red: Color::Rgb(0xFF, 0x2A, 0x4A),
    blue: Color::Rgb(0x4A, 0x9E, 0xFF),

    text: Color::Rgb(0xC8, 0xD0, 0xDC),
    muted: Color::Rgb(0x6A, 0x74, 0x88),
    faint: Color::Rgb(0x3A, 0x40, 0x52),
};

impl Theme {
    pub fn base(&self) -> Style {
        Style::new().bg(self.bg).fg(self.text)
    }
    pub fn panel_style(&self) -> Style {
        Style::new().bg(self.panel).fg(self.text)
    }
    pub fn title_active(&self) -> Style {
        Style::new().fg(self.cyan).add_modifier(Modifier::BOLD)
    }
    pub fn title_inactive(&self) -> Style {
        Style::new().fg(self.muted)
    }
    pub fn border_active(&self) -> Style {
        Style::new().fg(self.cyan)
    }
    pub fn border_inactive(&self) -> Style {
        Style::new().fg(self.faint)
    }
    pub fn border_modal(&self) -> Style {
        Style::new().fg(self.magenta)
    }
    pub fn border_danger(&self) -> Style {
        Style::new().fg(self.red).add_modifier(Modifier::BOLD)
    }
    pub fn ok(&self) -> Style {
        Style::new().fg(self.green)
    }
    pub fn warn(&self) -> Style {
        Style::new().fg(self.yellow)
    }
    pub fn err(&self) -> Style {
        Style::new().fg(self.red)
    }
    pub fn muted_style(&self) -> Style {
        Style::new().fg(self.muted)
    }
    pub fn hint(&self) -> Style {
        Style::new().fg(self.yellow)
    }

    /// Fixed identity color per stack, so a stack is recognizable everywhere
    /// (tables, logs, tickers) by hue alone.
    pub fn stack_color(&self, stack_name: &str) -> Color {
        match stack_name {
            "platform" => self.cyan,
            "media" => Color::Rgb(0xFF, 0x8A, 0x2A), // amber
            "downloader" => self.blue,
            "syncthing" => self.green,
            _ => self.magenta,
        }
    }
}
