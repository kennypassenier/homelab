//! Key-event handling for the homelab client TUI.
//!
//! `handle_key_event` is the single entry point called from the event loop.
//! It returns an `EventOutcome` telling the caller what to do next.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, LogLevelFilter, Tab};
use crate::blast_radius::{
    ActiveModal, AppCreationStep, AppCreationWizardState, DefaultServiceOption,
    SshAddStep, SshAddWizardState,
};

/// What the event loop should do after processing a key.
pub enum EventOutcome {
    /// Normal operation — continue the loop.
    Continue,
    /// User requested quit — break the loop.
    Quit,
    /// A create/delete action completed — reload stacks then continue.
    Reload,
}

/// Dispatch a key press to the appropriate handler and return the outcome.
///
/// Only `KeyEventKind::Press` events produce effects; all others are no-ops.
pub fn handle_key_event(app: &mut App, key: KeyEvent) -> EventOutcome {
    if key.kind != KeyEventKind::Press {
        return EventOutcome::Continue;
    }

    // ── Active modal takes full input priority ───────────────────────────────
    //
    // Each arm either mutates the modal state in place, or signals via the
    // returned WizardOutcome / needs_reload flag that the modal should close
    // after the borrow ends.
    let mut needs_reload = false;

    match &mut app.modal {
        ActiveModal::DeleteConfirmation { app_name, input } => {
            if key.code == KeyCode::Esc {
                app.modal = ActiveModal::None;
            } else if key.code == KeyCode::Enter {
                if input.value() == app_name {
                    let path = format!("stacks/{}", app_name);
                    let _ = std::fs::remove_dir_all(&path);
                    let _ = crate::gitops::commit_and_push(".", &format!("Delete {}", app_name));
                    // Reset selected index before closing modal so subsequent
                    // reload doesn't index into an out-of-range position
                    app.stacks = App::load_stacks();
                    if app.selected_stack >= app.stacks.len() && !app.stacks.is_empty() {
                        app.selected_stack = app.stacks.len() - 1;
                    }
                    app.modal = ActiveModal::None;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        ActiveModal::DeleteAppConfirmation {
            stack_name,
            app_name,
            input,
        } => {
            if key.code == KeyCode::Esc {
                app.modal = ActiveModal::None;
            } else if key.code == KeyCode::Enter {
                if input.value() == app_name {
                    let path = format!("stacks/{}/{}", stack_name, app_name);
                    let _ = std::fs::remove_dir_all(&path);
                    let _ = crate::gitops::commit_and_push(
                        ".",
                        &format!("Delete app {} from stack {}", app_name, stack_name),
                    );
                    app.modal = ActiveModal::None;
                    needs_reload = true;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        ActiveModal::AppCreationWizard(state) => {
            match handle_wizard(state, key) {
                WizardOutcome::Continue => {}
                WizardOutcome::Close => {
                    app.modal = ActiveModal::None;
                }
                WizardOutcome::Reload => {
                    // Modal stays open at the Done step; caller reloads stacks
                    needs_reload = true;
                }
            }
        }

        ActiveModal::SshAddWizard(state) => {
            match handle_ssh_wizard(state, key) {
                WizardOutcome::Continue => {}
                WizardOutcome::Close => {
                    app.modal = ActiveModal::None;
                }
                WizardOutcome::Reload => {}
            }
        }

        ActiveModal::None => {
            return handle_navigation(app, key);
        }
    }

    if needs_reload {
        return EventOutcome::Reload;
    }
    EventOutcome::Continue
}

// ── Wizard ───────────────────────────────────────────────────────────────────

/// Outcome from processing one key in the app-creation wizard.
enum WizardOutcome {
    Continue,
    Close,
    Reload,
}

/// Advances the wizard state machine for the given key press.
fn handle_wizard(state: &mut AppCreationWizardState, key: KeyEvent) -> WizardOutcome {
    match &mut state.step {
        AppCreationStep::Name { input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let name = input.value().trim().to_string();
                if name.is_empty() {
                    *error = Some("App name cannot be empty".to_string());
                } else if std::path::Path::new(&format!("stacks/{}/{}", state.stack_name, name))
                    .exists()
                {
                    *error = Some("App already exists in this stack".to_string());
                } else {
                    state.app_name = Some(name);
                    state.step = AppCreationStep::DockerPrompt {
                        input: Input::default(),
                        error: None,
                    };
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        AppCreationStep::DockerPrompt { input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let image = input.value().trim().to_string();
                if image.is_empty() {
                    *error = Some("Docker image cannot be empty".to_string());
                } else {
                    state.docker_image = Some(image);
                    let options = vec![
                        DefaultServiceOption {
                            label: "Watchtower",
                            description: "Auto-update",
                        },
                        DefaultServiceOption {
                            label: "Promtail",
                            description: "Log shipping",
                        },
                        DefaultServiceOption {
                            label: "Traefik",
                            description: "Reverse proxy (Docker stacks only)",
                        },
                    ];
                    let selected = vec![true, true, true];
                    state.step = AppCreationStep::DefaultsMultiselect { options, selected };
                    state.multiselect_cursor = 0;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        AppCreationStep::DefaultsMultiselect { options, selected } => {
            let len = options.len();
            let cursor = &mut state.multiselect_cursor;
            match key.code {
                KeyCode::Esc => return WizardOutcome::Close,
                KeyCode::Up => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Down => {
                    if *cursor + 1 < len {
                        *cursor += 1;
                    }
                }
                KeyCode::Char(' ') => {
                    selected[*cursor] = !selected[*cursor];
                }
                KeyCode::Enter => {
                    let app_name = state.app_name.as_deref().unwrap_or("");
                    let docker_image = state.docker_image.as_deref().unwrap_or("");
                    let mut summary = format!(
                        "Stack: {}\nApp: {}\nDocker image: {}\n\nDefault services:\n",
                        state.stack_name, app_name, docker_image
                    );
                    for (i, opt) in options.iter().enumerate() {
                        if selected[i] {
                            summary.push_str(&format!("- {}\n", opt.label));
                        }
                    }
                    state.step = AppCreationStep::Review { summary };
                }
                _ => {}
            }
        }

        AppCreationStep::Review { summary: _ } => {
            match key.code {
                KeyCode::Esc => return WizardOutcome::Close,
                KeyCode::Enter => {
                    let stack_name = state.stack_name.clone();
                    let app_name = state.app_name.as_deref().unwrap_or("newapp").to_string();
                    let _docker_image = state
                        .docker_image
                        .as_deref()
                        .unwrap_or("nginx:latest")
                        .to_string();

                    let mac = crate::scaffold::generate_mac_address();
                    let domain = format!("{}.local", app_name);
                    let tmpl = crate::scaffold::AppServiceTemplate {
                        app_name: &app_name,
                        mac_address: &mac,
                        domain_name: &domain,
                    };

                    // For the Review step the DefaultsMultiselect selections are already
                    // baked into the summary string; we default all to true here.
                    let compose =
                        crate::scaffold::scaffold_stack_with_services(&tmpl, true, true, true);
                    let _ = crate::scaffold::create_app_dirs(&stack_name, &app_name);
                    let compose_path =
                        format!("stacks/{}/{}/docker-compose.yml", stack_name, app_name);
                    let _ = std::fs::write(&compose_path, compose);

                    state.step = AppCreationStep::Done;
                    return WizardOutcome::Reload;
                }
                _ => {}
            }
        }

        AppCreationStep::Done => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return WizardOutcome::Close;
            }
        }
    }

    WizardOutcome::Continue
}

// ── Normal navigation (no modal open) ────────────────────────────────────────

fn handle_navigation(app: &mut App, key: KeyEvent) -> EventOutcome {
    match app.active_tab() {
        Tab::Scaffolding => handle_scaffolding_nav(app, key),
        Tab::Logs => handle_logs_nav(app, key),
        Tab::HostManagement => handle_host_management_nav(app, key),
        _ => handle_generic_nav(app, key),
    }
}

/// Handles key presses on the Host Management tab.
fn handle_host_management_nav(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Char('a') => {
            app.modal = ActiveModal::SshAddWizard(SshAddWizardState {
                step: SshAddStep::Alias {
                    input: Input::default(),
                    error: None,
                },
            });
            EventOutcome::Continue
        }
        _ => handle_generic_nav(app, key),
    }
}

/// Handles key presses on the Logs tab: Up/Down scroll, End to return to live.
fn handle_logs_nav(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Up => {
            // Clamp at (total - 1) so we never scroll past the oldest line.
            let max_scroll = app.logs.len().saturating_sub(1);
            if app.log_scroll < max_scroll {
                app.log_scroll += 1;
            }
        }
        KeyCode::Down => {
            app.log_scroll = app.log_scroll.saturating_sub(1);
        }
        KeyCode::End => {
            app.log_scroll = 0;
        }
        // Source legend horizontal scroll.
        KeyCode::Left => {
            app.log_source_scroll = app.log_source_scroll.saturating_sub(1);
        }
        KeyCode::Right => {
            use crate::app::LOG_SOURCES;
            if app.log_source_scroll + 1 < LOG_SOURCES.len() {
                app.log_source_scroll += 1;
            }
        }
        // Cycle log-level filter.
        KeyCode::Char('f') => {
            app.log_level_filter = app.log_level_filter.next();
            // Reset scroll so we don't overshoot the filtered view.
            app.log_scroll = 0;
        }
        KeyCode::Tab => app.tab_right(),
        KeyCode::BackTab => app.tab_left(),
        KeyCode::Char('q') => return EventOutcome::Quit,
        _ => {}
    }
    EventOutcome::Continue
}

fn handle_generic_nav(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Char('q') => EventOutcome::Quit,
        KeyCode::Tab => {
            app.tab_right();
            EventOutcome::Continue
        }
        KeyCode::BackTab => {
            app.tab_left();
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

fn handle_scaffolding_nav(app: &mut App, key: KeyEvent) -> EventOutcome {
    if app.stacks.is_empty() {
        return handle_generic_nav(app, key);
    }

    match key.code {
        KeyCode::Up => match app.column_focus {
            0 => {
                if app.selected_stack > 0 {
                    app.selected_stack -= 1;
                    if app.selected_stack < app.stack_scroll {
                        app.stack_scroll = app.selected_stack;
                    }
                }
            }
            1 => {
                let d = &mut app.stack_dropdowns[app.selected_stack];
                if d.selected_option > 0 {
                    d.selected_option -= 1;
                }
            }
            2 => {
                let d = &mut app.stack_dropdowns[app.selected_stack];
                if d.selected_option > 2 {
                    d.selected_option -= 1;
                }
            }
            _ => {}
        },

        KeyCode::Down => match app.column_focus {
            0 => {
                if app.selected_stack + 1 < app.stacks.len() {
                    app.selected_stack += 1;
                    let visible = app.stacks.len().min(20);
                    if app.selected_stack >= app.stack_scroll + visible {
                        app.stack_scroll += 1;
                    }
                }
            }
            1 => {
                let d = &mut app.stack_dropdowns[app.selected_stack];
                if d.selected_option + 1 < 3 {
                    d.selected_option += 1;
                }
            }
            2 => {
                let d = &mut app.stack_dropdowns[app.selected_stack];
                let mut max = 2 + d.apps.len();
                for ad in &d.app_dropdowns {
                    if ad.expanded {
                        max += 2; // each expanded app shows 2 sub-items
                    }
                }
                if d.selected_option + 1 < max {
                    d.selected_option += 1;
                }
            }
            _ => {}
        },

        KeyCode::Left => match app.column_focus {
            2 => {
                app.column_focus = 1;
                app.stack_dropdowns[app.selected_stack].selected_option = 0;
            }
            1 => {
                app.column_focus = 0;
            }
            _ => {}
        },

        KeyCode::Right => match app.column_focus {
            0 => {
                app.column_focus = 1;
            }
            1 => {
                app.column_focus = 2;
                let d = &app.stack_dropdowns[app.selected_stack];
                if !d.apps.is_empty() {
                    app.stack_dropdowns[app.selected_stack].selected_option = 2;
                }
            }
            _ => {}
        },

        KeyCode::Tab => {
            app.tab_right();
        }
        KeyCode::BackTab => {
            app.tab_left();
        }

        KeyCode::Enter | KeyCode::Char(' ') => {
            return handle_scaffolding_enter(app);
        }

        KeyCode::Esc => {
            let d = &mut app.stack_dropdowns[app.selected_stack];
            d.expanded = false;
            for ad in d.app_dropdowns.iter_mut() {
                ad.expanded = false;
            }
        }

        KeyCode::Char('q') => return EventOutcome::Quit,

        _ => {}
    }

    EventOutcome::Continue
}

fn handle_scaffolding_enter(app: &mut App) -> EventOutcome {
    match app.column_focus {
        0 => {
            let d = &mut app.stack_dropdowns[app.selected_stack];
            d.expanded = !d.expanded;
        }
        1 => {
            let stack_name = app.stacks[app.selected_stack].clone();
            let opt = app.stack_dropdowns[app.selected_stack].selected_option;
            match opt {
                0 => {
                    // Open the app-creation wizard for this stack
                    app.modal = ActiveModal::AppCreationWizard(AppCreationWizardState {
                        stack_name,
                        app_name: None,
                        docker_image: None,
                        step: AppCreationStep::Name {
                            input: Input::default(),
                            error: None,
                        },
                        multiselect_cursor: 0,
                    });
                }
                1 => {
                    // Confirm before deleting the entire stack
                    app.modal = ActiveModal::DeleteConfirmation {
                        app_name: stack_name,
                        input: Input::default(),
                    };
                }
                2 => {
                    // Stack-level config management (pre-sync.sh, lxc-compose.yml, etc.)
                    // TODO: open stack config editor modal
                }
                _ => {}
            }
        }
        2 => {
            let stack_name = app.stacks[app.selected_stack].clone();
            let idx = app.stack_dropdowns[app.selected_stack].selected_option;
            let d = &mut app.stack_dropdowns[app.selected_stack];

            let mut cursor: usize = 2;
            for (i, ad) in d.app_dropdowns.iter_mut().enumerate() {
                if idx == cursor {
                    if ad.expanded {
                        // Already expanded — first sub-item would be "Edit Config"; treat
                        // pressing Enter on the app row as toggle-collapse
                        ad.expanded = false;
                    } else {
                        ad.expanded = true;
                    }
                    return EventOutcome::Continue;
                }
                cursor += 1;
                if ad.expanded {
                    if idx == cursor {
                        // "Edit Config" sub-item — not implemented yet
                        return EventOutcome::Continue;
                    }
                    cursor += 1;
                    if idx == cursor {
                        // "Delete App" sub-item
                        app.modal = ActiveModal::DeleteAppConfirmation {
                            stack_name: stack_name.clone(),
                            app_name: d.apps[i].clone(),
                            input: Input::default(),
                        };
                        return EventOutcome::Continue;
                    }
                    cursor += 1;
                }
            }
        }
        _ => {}
    }

    EventOutcome::Continue
}

// ── SSH Add Wizard event handler ──────────────────────────────────────────────

/// Advances the SSH alias wizard state machine.
fn handle_ssh_wizard(state: &mut SshAddWizardState, key: KeyEvent) -> WizardOutcome {
    use crate::ssh_config::SshEntry;

    match &mut state.step {
        SshAddStep::Alias { input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let alias = input.value().trim().to_string();
                if alias.is_empty() {
                    *error = Some("Alias cannot be empty".to_string());
                } else {
                    state.step = SshAddStep::Ip {
                        alias,
                        input: Input::default(),
                        error: None,
                    };
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        SshAddStep::Ip { alias, input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let ip = input.value().trim().to_string();
                if ip.is_empty() {
                    *error = Some("IP address cannot be empty".to_string());
                } else {
                    let alias = alias.clone();
                    state.step = SshAddStep::User {
                        alias,
                        ip,
                        input: Input::default(),
                    };
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        SshAddStep::User { alias, ip, input } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let user = {
                    let v = input.value().trim().to_string();
                    if v.is_empty() { "root".to_string() } else { v }
                };
                let entry = SshEntry {
                    host: alias.clone(),
                    hostname: ip.clone(),
                    user,
                    port: 22,
                };
                let alias_done = alias.clone();
                let _ = crate::ssh_config::upsert_ssh_entry(&entry);
                state.step = SshAddStep::Done { alias: alias_done };
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        SshAddStep::Done { .. } => {
            // Any key closes the wizard
            return WizardOutcome::Close;
        }
    }

    WizardOutcome::Continue
}
