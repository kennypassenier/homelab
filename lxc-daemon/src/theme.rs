use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub const BG: Color = Color::Black;
    pub const FG: Color = Color::White;
    pub const CYAN: Color = Color::Cyan;
    pub const MAGENTA: Color = Color::Magenta;
    pub const GREEN: Color = Color::Green;
    pub const RED: Color = Color::Red;
    pub const YELLOW: Color = Color::Yellow;
}

impl Default for Theme {
    fn default() -> Self {
        Self
    }
}

pub fn header_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::Cyan)
}

pub fn active_tab_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::Cyan)
        .bg(Color::DarkGray)
}

pub fn inactive_tab_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn info_widget_style() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn title_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::Cyan)
}
