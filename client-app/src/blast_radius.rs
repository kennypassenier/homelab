//! Blast Radius protection modal for destructive actions in Homelab Client.
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use tui_input::Input;

/// Enum for active modal state.
pub enum ActiveModal {
    DeleteConfirmation {
        app_name: String,
        input: Input,
    },
    DeleteAppConfirmation {
        stack_name: String,
        app_name: String,
        input: Input,
    },
    AppCreationWizard(AppCreationWizardState),
    None,
}

/// State for the multi-step app creation wizard
pub struct AppCreationWizardState {
    pub stack_name: String,
    pub step: AppCreationStep,
    pub multiselect_cursor: usize, // for DefaultsMultiselect step
}

pub enum AppCreationStep {
    Name {
        input: Input,
        error: Option<String>,
    },
    DockerPrompt {
        selected: bool,
    },
    DefaultsMultiselect {
        options: Vec<DefaultServiceOption>,
        selected: Vec<bool>,
    },
    // Review, Done to be added
}

pub struct DefaultServiceOption {
    pub label: &'static str,
    pub description: &'static str,
}

/// Draws the app creation wizard modal (step-based)
pub fn draw_app_creation_wizard(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &AppCreationWizardState,
) {
    use ratatui::{style::*, widgets::*};
    let popup_area = ratatui::layout::Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 10,
    };
    f.render_widget(Clear, popup_area);
    match &state.step {
        AppCreationStep::Name { input, error } => {
            let block = Block::default()
                .title("Create New App")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Enter new app name for stack '{}':\n> {}\n\n[Enter to continue, ESC to cancel]",
                state.stack_name,
                input.value()
            );
            if let Some(err) = error {
                text.push_str(&format!("\n\n[Error: {}]", err));
            }
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        AppCreationStep::DockerPrompt { selected } => {
            let block = Block::default()
                .title("Docker Prompt")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Docker image to use:\n> {}\n\n[Enter to continue, ESC to cancel]",
                input.value()
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        AppCreationStep::DefaultsMultiselect { options, selected } => {
            let block = Block::default()
                .title("Default Services")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = String::from("Select default containers to add:\n\n");
            for (i, opt) in options.iter().enumerate() {
                let mark = if selected[i] { "[x]" } else { "[ ]" };
                let cursor = if i == state.multiselect_cursor {
                    "⮞"
                } else {
                    "  "
                };
                text.push_str(&format!("  {} {} {}\n", cursor, mark, opt.label));
            }
            text.push_str("\n[↑/↓ to move, Space to toggle, Enter to confirm, ESC to cancel]");
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
    }
}

/// Draws a warning modal with a red border and input field.
pub fn draw_warning_modal(f: &mut ratatui::Frame, area: Rect, app_name: &str, input: &Input) {
    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 8,
    };
    f.render_widget(Clear, popup_area); // darken background
    let block = Block::default()
        .title("DANGER")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    let text = format!(
        "DANGER: Type the exact name of the app to delete it.\n\nApp: {}\n> {}\n\n[ESC to cancel]",
        app_name,
        input.value()
    );
    let para = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));
    f.render_widget(para, popup_area);
}

/// Draws a warning modal for deleting an app (shows stack and app)
pub fn draw_delete_app_modal(
    f: &mut ratatui::Frame,
    area: Rect,
    stack_name: &str,
    app_name: &str,
    input: &Input,
) {
    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 9,
    };
    f.render_widget(Clear, popup_area);
    let block = Block::default()
        .title("DANGER")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    let text = format!(
        "DANGER: Type the exact app name to delete it.\n\nStack: {}\nApp: {}\n> {}\n\n[ESC to cancel]",
        stack_name,
        app_name,
        input.value()
    );
    let para = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));
    f.render_widget(para, popup_area);
}
