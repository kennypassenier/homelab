//! Blast Radius protection modal for destructive actions in Homelab Client.
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use tui_input::Input;

/// Enum for active modal state.
pub enum ActiveModal {
    DeleteConfirmation { app_name: String, input: Input },
    None,
}

/// Draws a warning modal with a red border and input field.
pub fn draw_warning_modal(f: &mut ratatui::Frame, area: Rect, app_name: &str, input: &Input) {
    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 7,
    };
    f.render_widget(Clear, popup_area); // darken background
    let block = Block::default()
        .title("DANGER")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    let text = format!(
        "DANGER: Type the exact name of the app to delete it.\n\nApp: {}\n> {}",
        app_name,
        input.value()
    );
    let para = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));
    f.render_widget(para, popup_area);
}
