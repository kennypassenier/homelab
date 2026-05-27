use ratatui::style::{Color, Modifier, Style};

// Theme struct - will be initialized later in main()
#[derive(Debug)]
pub struct Theme {
    pub light_blue_dark_blue: [(Style, Color); 2],
    pub dark_gray_white: [(Style, Color); 2],
}

impl Theme {
    pub fn new() -> Self {
        // Hardcoded values to avoid Style::default() in const context
        Self {
            light_blue_dark_blue: [
                (
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .bg(Color::DarkGray)
                        .fg(Color::Blue),
                    Color::DarkGray,
                ),
                (
                    Style::default().bg(Color::DarkGray).fg(Color::LightBlue),
                    Color::DarkGray,
                ),
            ],
            dark_gray_white: [
                (
                    Style::default().bg(Color::DarkGray).fg(Color::White),
                    Color::DarkGray,
                ),
                (
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                    Color::DarkGray,
                ),
            ],
        }
    }
}
