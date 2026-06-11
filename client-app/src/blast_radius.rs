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
    AppConfigEditor(AppConfigEditorState),
    StackCreationWizard(StackCreationWizardState),
    StackConfigEditor(StackConfigEditorState),
    OperationProgress(OperationProgressState),
    SshAddWizard(SshAddWizardState),
    None,
}

/// State for the multi-step app creation wizard
pub struct AppCreationWizardState {
    pub stack_name: String,
    pub app_name: Option<String>,
    pub docker_image: Option<String>,
    pub selected_defaults: Vec<String>,
    pub subdomain: Option<String>,
    pub step: AppCreationStep,
    pub multiselect_cursor: usize, // for DefaultsMultiselect step
}

pub enum AppCreationStep {
    Name {
        input: Input,
        error: Option<String>,
    },
    DockerPrompt {
        input: Input,
        error: Option<String>,
    },
    DefaultsMultiselect {
        options: Vec<DefaultServiceOption>,
        selected: Vec<bool>,
    },
    SubdomainInput {
        input: Input,
        error: Option<String>,
        domain: String,
    },
    Review {
        summary: String,
    },
    Done,
}

pub struct DefaultServiceOption {
    pub label: &'static str,
    pub description: &'static str,
}

pub struct StackCreationWizardState {
    pub stack_name: Option<String>,
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub autostart: bool,
    pub startup_order: u32,
    pub vmid: u32,
    pub step: StackCreationStep,
}

pub enum StackCreationStep {
    Name { input: Input, error: Option<String> },
    CpuSelect,
    MemorySelect,
    DiskInput { input: Input, error: Option<String> },
    AutoStartSelect,
    BootOrderSelect,
    VmidInput { input: Input, error: Option<String> },
    Review { summary: String },
    Done,
}

pub struct StackConfigEditorState {
    pub stack_name: String,
    pub vmid: u32,
    pub hostname: String,
    pub hwaddr: String,
    pub bridge: String,
    pub ip_mode: String,
    pub reserved_ipv4: Option<String>,
    pub autostart: bool,
    pub startup_order: u32,
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub swap_mb: u32,
    pub cpu_limit: Option<f64>,
    pub cpu_units: u32,
    pub vlan_tag: Option<u16>,
    pub firewall: bool,
    pub ip_mode_v6: Option<String>,
    pub disk_gb: u32,
    pub rootfs_pool: String,
    pub appdata_backup: bool,
    pub appdata_read_only: bool,
    pub deploy_enabled: bool,
    pub activated_at: Option<String>,
    pub timezone: String,
    pub protection: bool,
    pub tags: Vec<String>,
    pub pre_sync_exists: bool,
    pub selected_field: usize,
    pub error: Option<String>,
    pub step: StackConfigEditorStep,
}

pub enum StackConfigEditorStep {
    Overview,
    EditHostname { input: Input },
    EditHwaddr { input: Input },
    EditReservedIpv4 { input: Input },
    Done { summary: String },
}

pub struct AppConfigEditorState {
    pub stack_name: String,
    pub app_name: String,
    pub docker_image: String,
    pub selected_field: usize,
    pub error: Option<String>,
    pub step: AppConfigEditorStep,
}

pub enum AppConfigEditorStep {
    Overview,
    EditDockerImage { input: Input },
    Done { summary: String },
}

pub struct OperationProgressState {
    pub title: String,
    pub phase: String,
    pub entries: Vec<OperationEntry>,
    pub summary: String,
}

pub struct OperationEntry {
    pub name: String,
    pub status: String,
    pub detail: String,
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
        AppCreationStep::DockerPrompt { input, error } => {
            let block = Block::default()
                .title("Docker Image")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Enter Docker image for app:\n> {}\n\n[Enter to continue, ESC to cancel]",
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
        AppCreationStep::SubdomainInput {
            input,
            error,
            domain,
        } => {
            let block = Block::default()
                .title("Subdomain (Traefik Routing)")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let preview = if input.value().is_empty() {
                format!("(will be: ?.{})", domain)
            } else {
                format!("(will be: {}.{})", input.value(), domain)
            };
            let mut text = format!(
                "Enter subdomain for Traefik routing:\n> {}\n{}\n\n[Enter to continue, ESC to cancel]",
                input.value(),
                preview
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
        AppCreationStep::Review { summary } => {
            let block = Block::default()
                .title("Review & Confirm")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
            let text = format!(
                "Review your new app:\n\n{}\n\n[Enter to create, ESC to cancel]",
                summary
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::Green));
            f.render_widget(para, popup_area);
        }
        AppCreationStep::Done => {
            let block = Block::default()
                .title("App Created!")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
            let text = "App created and docker-compose.yml written!\n\n[ESC to close]";
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Green));
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
                text.push_str(&format!(
                    "  {} {} {}  - {}\n",
                    cursor, mark, opt.label, opt.description
                ));
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

pub fn draw_stack_creation_wizard(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &StackCreationWizardState,
) {
    use ratatui::{style::*, widgets::*};
    let popup_height = match &state.step {
        StackCreationStep::Review { .. } => area.height.saturating_sub(4).clamp(14, 20),
        _ => 10,
    };
    let popup_area = ratatui::layout::Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: popup_height,
    };
    f.render_widget(Clear, popup_area);

    match &state.step {
        StackCreationStep::Name { input, error } => {
            let block = Block::default()
                .title("Create New Stack")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Enter new stack name:\n> {}\n\n[Enter to continue, ESC to cancel]",
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
        StackCreationStep::CpuSelect => {
            let block = Block::default()
                .title("CPU Cores")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let text = format!(
                "Select CPU cores (1-8):\n\n  cores: {}\n\n[←/→ or +/- adjust, Enter continue, ESC cancel]",
                state.cpu_cores
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        StackCreationStep::MemorySelect => {
            let block = Block::default()
                .title("Memory")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let gib = state.memory_mb as f32 / 1024.0;
            let text = format!(
                "Select memory (512 MiB steps):\n\n  memory: {:.1} GiB ({} MiB)\n\n[←/→ or +/- adjust, Enter continue, ESC cancel]",
                gib, state.memory_mb
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        StackCreationStep::DiskInput { input, error } => {
            let block = Block::default()
                .title("Disk Size")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Enter root disk size (GiB integer):\n> {}\n\n[Enter continue, ESC cancel]",
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
        StackCreationStep::AutoStartSelect => {
            let block = Block::default()
                .title("Auto Start")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let text = format!(
                "Start this LXC automatically on host boot:\n\n  autostart: {}\n\n[←/→ or +/- toggle, Enter continue, ESC cancel]",
                state.autostart
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        StackCreationStep::BootOrderSelect => {
            let block = Block::default()
                .title("Boot Order")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let text = format!(
                "Set boot order priority (higher means later / lower priority):\n\n  boot order: {}\n\nDefault 90 keeps stacks behind critical infra.\n\n[←/→ or +/- adjust, Enter continue, ESC cancel]",
                state.startup_order
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(para, popup_area);
        }
        StackCreationStep::VmidInput { input, error } => {
            let block = Block::default()
                .title("Container ID (VMID)")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            let mut text = format!(
                "Enter VMID for this LXC container (101-354, e.g., 105):\n> {}\n\n[Enter continue, ESC cancel]",
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
        StackCreationStep::Review { summary } => {
            let block = Block::default()
                .title("Review & Confirm")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
            let text = format!(
                "Review your new stack:\n\n{}\n\n  [Enter] Create stack    [ESC] Cancel",
                summary
            );
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Green));
            f.render_widget(para, popup_area);
        }
        StackCreationStep::Done => {
            let block = Block::default()
                .title("Stack Created")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
            let text = "Stack scaffold created with core apps.\n\n[Enter or ESC to close]";
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Green));
            f.render_widget(para, popup_area);
        }
    }
}

pub fn draw_stack_config_editor(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &StackConfigEditorState,
) {
    use ratatui::{style::*, widgets::*};

    let popup_area = ratatui::layout::Rect {
        x: area.width / 6,
        y: area.height / 6,
        width: area.width * 2 / 3,
        height: area.height * 2 / 3,
    };
    f.render_widget(Clear, popup_area);

    match &state.step {
        StackConfigEditorStep::Overview => {
            let block = Block::default()
                .title(format!("Stack Config :: {}", state.stack_name))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );

            let fields = [
                format!("Deploy enabled: {}", state.deploy_enabled),
                format!("CPU cores: {}", state.cpu_cores),
                format!(
                    "Memory: {:.1} GiB ({} MiB)",
                    state.memory_mb as f32 / 1024.0,
                    state.memory_mb
                ),
                format!("Disk: {} GiB", state.disk_gb),
                format!("Hostname: {}", state.hostname),
                format!("MAC address: {}", state.hwaddr),
                format!("IP mode: {}", state.ip_mode),
                format!(
                    "Reserved IPv4: {}",
                    state.reserved_ipv4.as_deref().unwrap_or("(unset)")
                ),
                format!("Autostart: {}", state.autostart),
                format!("Boot order: {}", state.startup_order),
                "Sync DHCP reservation".to_string(),
                "Save and commit".to_string(),
            ];

            let mut text = format!(
                "VMID: {}\nBridge: {}\nPre-sync hook: {}\nActivated at: {}\n\n",
                state.vmid,
                state.bridge,
                if state.pre_sync_exists {
                    "present"
                } else {
                    "missing"
                },
                state.activated_at.as_deref().unwrap_or("null")
            );

            for (index, field) in fields.iter().enumerate() {
                let cursor = if index == state.selected_field {
                    ">"
                } else {
                    " "
                };
                text.push_str(&format!("{} {}\n", cursor, field));
            }

            text.push_str("\n[↑/↓] select  [←/→ +/-] adjust  [Enter] edit/action  [Esc] close");
            if let Some(err) = &state.error {
                text.push_str(&format!("\n\nError: {}", err));
            }

            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        StackConfigEditorStep::EditHostname { input } => {
            let para = Paragraph::new(format!(
                "Enter hostname for stack '{}':\n> {}\n\n[Enter save, Esc cancel]{}",
                state.stack_name,
                input.value(),
                state
                    .error
                    .as_deref()
                    .map(|e| format!("\n\nError: {}", e))
                    .unwrap_or_default()
            ))
            .block(
                Block::default()
                    .title("Edit Hostname")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        StackConfigEditorStep::EditHwaddr { input } => {
            let para = Paragraph::new(format!(
                "Enter MAC address for stack '{}':\n> {}\n\n[Enter save, Esc cancel]{}",
                state.stack_name,
                input.value(),
                state
                    .error
                    .as_deref()
                    .map(|e| format!("\n\nError: {}", e))
                    .unwrap_or_default()
            ))
            .block(
                Block::default()
                    .title("Edit MAC Address")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        StackConfigEditorStep::EditReservedIpv4 { input } => {
            let para = Paragraph::new(format!(
                "Enter reserved IPv4 for stack '{}':\n> {}\n\nLeave blank to clear it.\n\n[Enter save, Esc cancel]{}",
                state.stack_name,
                input.value(),
                state
                    .error
                    .as_deref()
                    .map(|e| format!("\n\nError: {}", e))
                    .unwrap_or_default()
            ))
            .block(
                Block::default()
                    .title("Edit Reserved IPv4")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        StackConfigEditorStep::Done { summary } => {
            let para = Paragraph::new(format!("{}\n\n[Enter/Esc] close", summary))
                .block(
                    Block::default()
                        .title("Stack Config Saved")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Green)),
                )
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Green));
            f.render_widget(para, popup_area);
        }
    }
}

pub fn draw_app_config_editor(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &AppConfigEditorState,
) {
    use ratatui::{style::*, widgets::*};

    let popup_area = ratatui::layout::Rect {
        x: area.width / 5,
        y: area.height / 4,
        width: area.width * 3 / 5,
        height: area.height / 2,
    };
    f.render_widget(Clear, popup_area);

    match &state.step {
        AppConfigEditorStep::Overview => {
            let fields = [
                format!("Docker image: {}", state.docker_image),
                "Save and commit".to_string(),
            ];
            let mut text = format!("Stack: {}\nApp: {}\n\n", state.stack_name, state.app_name);
            for (index, field) in fields.iter().enumerate() {
                let cursor = if index == state.selected_field {
                    ">"
                } else {
                    " "
                };
                text.push_str(&format!("{} {}\n", cursor, field));
            }
            text.push_str("\n[↑/↓] select  [Enter] edit/save  [Esc] close");
            if let Some(err) = &state.error {
                text.push_str(&format!("\n\nError: {}", err));
            }

            let para = Paragraph::new(text)
                .block(
                    Block::default()
                        .title(format!(
                            "App Config :: {}/{}",
                            state.stack_name, state.app_name
                        ))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        AppConfigEditorStep::EditDockerImage { input } => {
            let para = Paragraph::new(format!(
                "Enter Docker image for app '{}':\n> {}\n\n[Enter save, Esc cancel]{}",
                state.app_name,
                input.value(),
                state
                    .error
                    .as_deref()
                    .map(|e| format!("\n\nError: {}", e))
                    .unwrap_or_default()
            ))
            .block(
                Block::default()
                    .title("Edit Docker Image")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));
            f.render_widget(para, popup_area);
        }
        AppConfigEditorStep::Done { summary } => {
            let para = Paragraph::new(format!("{}\n\n[Enter/Esc] close", summary))
                .block(
                    Block::default()
                        .title("App Config Saved")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Green)),
                )
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Green));
            f.render_widget(para, popup_area);
        }
    }
}

pub fn draw_operation_progress(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &OperationProgressState,
) {
    use ratatui::text::Line;
    use ratatui::{style::*, widgets::*};

    let popup_area = ratatui::layout::Rect {
        x: area.width / 8,
        y: area.height / 6,
        width: area.width * 3 / 4,
        height: area.height * 2 / 3,
    };
    f.render_widget(Clear, popup_area);

    let body_height = popup_area.height.saturating_sub(5);
    let rows = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(2),
            ratatui::layout::Constraint::Length(body_height),
            ratatui::layout::Constraint::Length(3),
        ])
        .split(popup_area);

    f.render_widget(
        Paragraph::new(format!("Phase: {}", state.phase)).block(
            Block::default()
                .title(format!(" {} ", state.title))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        ),
        rows[0],
    );

    let entries = if state.entries.is_empty() {
        vec![Line::from("  (no items)")]
    } else {
        state
            .entries
            .iter()
            .map(|entry| {
                Line::from(format!(
                    "  {}  {:<20}  {}",
                    entry.status, entry.name, entry.detail
                ))
            })
            .collect()
    };

    f.render_widget(
        Paragraph::new(entries).block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        rows[1],
    );

    f.render_widget(
        Paragraph::new(format!("{}\n[Enter/Esc] close", state.summary))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .style(Style::default().fg(Color::Rgb(170, 190, 190))),
        rows[2],
    );
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

// ── SSH Add Wizard ───────────────────────────────────────────────────────────

/// Multi-step state for the SSH alias creation wizard in the Host Management tab.
pub struct SshAddWizardState {
    pub step: SshAddStep,
}

/// Steps in the SSH add wizard.
pub enum SshAddStep {
    /// User types the Host alias (e.g. "lxc-media").
    Alias { input: Input, error: Option<String> },
    /// User types the IP address or hostname.
    Ip {
        alias: String,
        input: Input,
        error: Option<String>,
    },
    /// User types the SSH username (default: root).
    User {
        alias: String,
        ip: String,
        input: Input,
    },
    /// The upsert completed — show confirmation.
    Done { alias: String },
}

/// Draws the SSH add wizard as a centred popup overlay.
pub fn draw_ssh_add_wizard(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &SshAddWizardState,
) {
    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 9,
    };
    f.render_widget(Clear, popup_area);

    let (title, body) = match &state.step {
        SshAddStep::Alias { input, error } => {
            let err_line = error
                .as_deref()
                .map(|e| format!("\n{}", e))
                .unwrap_or_default();
            (
                " Add SSH Alias (1/3): Host alias ",
                format!(
                    "Enter the SSH alias (e.g. lxc-media):\n> {}{}\n\n[Enter] next  [Esc] cancel",
                    input.value(),
                    err_line,
                ),
            )
        }
        SshAddStep::Ip {
            alias,
            input,
            error,
        } => {
            let err_line = error
                .as_deref()
                .map(|e| format!("\n{}", e))
                .unwrap_or_default();
            (
                " Add SSH Alias (2/3): IP address ",
                format!(
                    "Alias: {}\nEnter the IP address:\n> {}{}\n\n[Enter] next  [Esc] cancel",
                    alias,
                    input.value(),
                    err_line,
                ),
            )
        }
        SshAddStep::User { alias, ip, input } => (
            " Add SSH Alias (3/3): Username ",
            format!(
                "Alias: {}  IP: {}\nEnter username (default: root):\n> {}\n\n[Enter] save  [Esc] cancel",
                alias,
                ip,
                input.value(),
            ),
        ),
        SshAddStep::Done { alias } => (
            " SSH Alias Saved ",
            format!(
                "Alias '{}' written to ~/.ssh/config.\nConnect with: ssh {}\n\n[Enter/Esc] close",
                alias, alias,
            ),
        ),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    let para = Paragraph::new(body)
        .block(block)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White));
    f.render_widget(para, popup_area);
}
