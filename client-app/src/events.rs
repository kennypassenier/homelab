//! Key-event handling for the homelab client TUI.
//!
//! `handle_key_event` is the single entry point called from the event loop.
//! It returns an `EventOutcome` telling the caller what to do next.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, Tab};
use crate::blast_radius::{
    ActiveModal, AppConfigEditorState, AppConfigEditorStep, AppCreationStep,
    AppCreationWizardState, DefaultServiceOption, OperationEntry, OperationProgressState,
    SshAddStep, SshAddWizardState, StackConfigEditorState, StackConfigEditorStep,
    StackCreationStep, StackCreationWizardState,
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

    let known_stacks = app.stacks.clone();

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
                    let tx = crate::transactions::begin("delete_stack", app_name).ok();
                    if let Some(ref path) = tx {
                        let _ = crate::transactions::record_phase(
                            path,
                            "delete_stack_scaffold",
                            "in_progress",
                            None,
                        );
                    }

                    match crate::stack_features::delete_stack(app_name) {
                        Ok(()) => {
                            if let Some(ref path) = tx {
                                let _ = crate::transactions::record_phase(
                                    path,
                                    "delete_stack_scaffold",
                                    "completed",
                                    None,
                                );
                                let _ = crate::transactions::record_phase(
                                    path,
                                    "git_push",
                                    "in_progress",
                                    None,
                                );
                            }

                            let _ = crate::gitops::commit_and_push(
                                ".",
                                &format!("Delete stack {}", app_name),
                            );

                            if let Some(ref path) = tx {
                                let _ = crate::transactions::record_phase(
                                    path,
                                    "git_push",
                                    "completed",
                                    None,
                                );
                                let _ = crate::transactions::finish(path, true);
                            }

                            app.modal = ActiveModal::None;
                            needs_reload = true;
                        }
                        Err(e) => {
                            if let Some(ref path) = tx {
                                let _ = crate::transactions::record_phase(
                                    path,
                                    "delete_stack_scaffold",
                                    "failed",
                                    Some(&e.to_string()),
                                );
                                let _ = crate::transactions::finish(path, false);
                            }
                            app.sync_status =
                                format!("Delete stack blocked for '{}': {}", app_name, e);
                        }
                    }
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
                    let delete_result = if crate::stack_features::is_core_app(app_name) {
                        crate::stack_features::delete_core_app_from_stack(stack_name, app_name)
                    } else {
                        crate::stack_features::delete_app_from_stack(stack_name, app_name)
                    };

                    match delete_result {
                        Ok(()) => {
                            let msg = if crate::stack_features::is_core_app(app_name) {
                                format!("Delete core app {} from stack {}", app_name, stack_name)
                            } else {
                                format!("Delete app {} from stack {}", app_name, stack_name)
                            };
                            let _ = crate::gitops::commit_and_push(".", &msg);
                            app.modal = ActiveModal::None;
                            needs_reload = true;
                        }
                        Err(e) => {
                            app.sync_status =
                                format!("Delete app blocked for '{}': {}", app_name, e);
                        }
                    }
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

        ActiveModal::AppConfigEditor(state) => match handle_app_config_editor(state, key) {
            WizardOutcome::Continue => {}
            WizardOutcome::Close => {
                app.modal = ActiveModal::None;
            }
            WizardOutcome::Reload => {
                needs_reload = true;
            }
        },

        ActiveModal::StackCreationWizard(state) => match handle_stack_wizard(state, key) {
            WizardOutcome::Continue => {}
            WizardOutcome::Close => {
                app.modal = ActiveModal::None;
            }
            WizardOutcome::Reload => {
                needs_reload = true;
            }
        },

        ActiveModal::StackConfigEditor(state) => {
            match handle_stack_config_editor(state, key, &known_stacks) {
                WizardOutcome::Continue => {}
                WizardOutcome::Close => {
                    app.modal = ActiveModal::None;
                }
                WizardOutcome::Reload => {}
            }
        }

        ActiveModal::OperationProgress(_) => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                app.modal = ActiveModal::None;
            }
        }

        ActiveModal::SshAddWizard(state) => match handle_ssh_wizard(state, key) {
            WizardOutcome::Continue => {}
            WizardOutcome::Close => {
                app.modal = ActiveModal::None;
            }
            WizardOutcome::Reload => {}
        },

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
                    state.selected_defaults.clear();
                    let mut summary = format!(
                        "Stack: {}\nApp: {}\nDocker image: {}\n\nDefault services:\n",
                        state.stack_name, app_name, docker_image
                    );
                    for (i, opt) in options.iter().enumerate() {
                        if selected[i] {
                            summary.push_str(&format!("- {}\n", opt.label));
                            state.selected_defaults.push(opt.label.to_string());
                        }
                    }
                    state.step = AppCreationStep::Review { summary };
                }
                _ => {}
            }
        }

        AppCreationStep::Review { summary: _ } => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Enter => {
                let stack_name = state.stack_name.clone();
                let app_name = state.app_name.as_deref().unwrap_or("newapp").to_string();
                let docker_image = state
                    .docker_image
                    .as_deref()
                    .unwrap_or("nginx:latest")
                    .to_string();

                let options = crate::stack_features::AddAppOptions {
                    include_watchtower: state.selected_defaults.iter().any(|x| x == "Watchtower"),
                    include_promtail: state.selected_defaults.iter().any(|x| x == "Promtail"),
                    include_traefik: state.selected_defaults.iter().any(|x| x == "Traefik"),
                };

                let _ = crate::stack_features::add_app_to_stack(
                    &stack_name,
                    &app_name,
                    &docker_image,
                    &options,
                );
                let _ = crate::gitops::commit_and_push(
                    ".",
                    &format!(
                        "feat(scaffold): add app {} to stack {}",
                        app_name, stack_name
                    ),
                );

                state.step = AppCreationStep::Done;
                return WizardOutcome::Reload;
            }
            _ => {}
        },

        AppCreationStep::Done => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return WizardOutcome::Close;
            }
        }
    }

    WizardOutcome::Continue
}

fn handle_stack_wizard(state: &mut StackCreationWizardState, key: KeyEvent) -> WizardOutcome {
    match &mut state.step {
        StackCreationStep::Name { input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let name = input.value().trim().to_string();
                if name.is_empty() {
                    *error = Some("Stack name cannot be empty".to_string());
                } else if std::path::Path::new(&format!("stacks/{}", name)).exists() {
                    *error = Some("Stack already exists".to_string());
                } else {
                    state.stack_name = Some(name.clone());
                    state.step = StackCreationStep::CpuSelect;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        StackCreationStep::CpuSelect => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Left | KeyCode::Char('-') => {
                state.cpu_cores = state.cpu_cores.saturating_sub(1).max(1);
            }
            KeyCode::Right | KeyCode::Char('+') => {
                state.cpu_cores = state.cpu_cores.saturating_add(1).min(8);
            }
            KeyCode::Enter => {
                state.step = StackCreationStep::MemorySelect;
            }
            _ => {}
        },

        StackCreationStep::MemorySelect => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Left | KeyCode::Char('-') => {
                state.memory_mb = state.memory_mb.saturating_sub(512).max(512);
            }
            KeyCode::Right | KeyCode::Char('+') => {
                state.memory_mb = state.memory_mb.saturating_add(512).min(65536);
            }
            KeyCode::Enter => {
                state.step = StackCreationStep::DiskInput {
                    input: Input::default(),
                    error: None,
                };
            }
            _ => {}
        },

        StackCreationStep::DiskInput { input, error } => {
            if key.code == KeyCode::Esc {
                return WizardOutcome::Close;
            } else if key.code == KeyCode::Enter {
                let raw = input.value().trim();
                match raw.parse::<u32>() {
                    Ok(v) if v >= 8 => {
                        state.disk_gb = v;
                        state.step = StackCreationStep::AutoStartSelect;
                    }
                    _ => {
                        *error = Some("disk must be an integer >= 8".to_string());
                    }
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }

        StackCreationStep::AutoStartSelect => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Left | KeyCode::Right | KeyCode::Char('-') | KeyCode::Char('+') => {
                state.autostart = !state.autostart;
            }
            KeyCode::Enter => {
                state.step = StackCreationStep::BootOrderSelect;
            }
            _ => {}
        },

        StackCreationStep::BootOrderSelect => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Left | KeyCode::Char('-') => {
                state.startup_order = state.startup_order.saturating_sub(5).max(5);
            }
            KeyCode::Right | KeyCode::Char('+') => {
                state.startup_order = state.startup_order.saturating_add(5).min(500);
            }
            KeyCode::Enter => {
                let stack_name = state.stack_name.as_deref().unwrap_or("new-stack");
                state.step = StackCreationStep::Review {
                    summary: format!(
                        "Stack: {}\nCPU cores: {}\nMemory: {:.1} GiB ({} MiB)\nDisk: {} GiB\nAutostart: {}\nBoot order: {}\nDeploy enabled: false (manual activation required)\n\nActions:\n- create stacks/{}/\n- create setup.sh\n- create lxc-compose.yml\n- scaffold missing core apps",
                        stack_name,
                        state.cpu_cores,
                        state.memory_mb as f32 / 1024.0,
                        state.memory_mb,
                        state.disk_gb,
                        state.autostart,
                        state.startup_order,
                        stack_name
                    ),
                };
            }
            _ => {}
        },

        StackCreationStep::Review { summary: _ } => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Enter => {
                let stack_name = state.stack_name.as_deref().unwrap_or("new-stack");
                let tx = crate::transactions::begin("add_stack", stack_name).ok();
                if let Some(ref path) = tx {
                    let _ = crate::transactions::record_phase(
                        path,
                        "scaffold_git_files",
                        "in_progress",
                        None,
                    );
                }

                match crate::stack_features::create_stack(stack_name) {
                    Ok(_) => {
                        if let Err(e) = crate::scaffold::set_stack_provisioning_defaults(
                            stack_name,
                            state.cpu_cores,
                            state.memory_mb,
                            state.disk_gb,
                            state.autostart,
                            state.startup_order,
                        ) {
                            if let Some(ref path) = tx {
                                let _ = crate::transactions::record_phase(
                                    path,
                                    "scaffold_git_files",
                                    "failed",
                                    Some(&e.to_string()),
                                );
                                let _ = crate::transactions::finish(path, false);
                            }
                            state.step = StackCreationStep::Name {
                                input: Input::default(),
                                error: Some(format!("provisioning defaults failed: {}", e)),
                            };
                            return WizardOutcome::Continue;
                        }

                        if let Some(ref path) = tx {
                            let _ = crate::transactions::record_phase(
                                path,
                                "scaffold_git_files",
                                "completed",
                                None,
                            );
                            let _ = crate::transactions::record_phase(
                                path,
                                "git_push",
                                "in_progress",
                                None,
                            );
                        }

                        let _ = crate::gitops::commit_and_push(
                            ".",
                            &format!("Create stack {} with core scaffold", stack_name),
                        );

                        if let Some(ref path) = tx {
                            let _ = crate::transactions::record_phase(
                                path,
                                "git_push",
                                "completed",
                                None,
                            );
                            let _ = crate::transactions::finish(path, true);
                        }

                        state.step = StackCreationStep::Done;
                        return WizardOutcome::Reload;
                    }
                    Err(e) => {
                        if let Some(ref path) = tx {
                            let _ = crate::transactions::record_phase(
                                path,
                                "scaffold_git_files",
                                "failed",
                                Some(&e.to_string()),
                            );
                            let _ = crate::transactions::finish(path, false);
                        }
                        state.step = StackCreationStep::Name {
                            input: Input::default(),
                            error: Some(format!("create failed: {}", e)),
                        };
                    }
                }
            }
            _ => {}
        },

        StackCreationStep::Done => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return WizardOutcome::Close;
            }
        }
    }

    WizardOutcome::Continue
}

fn handle_stack_config_editor(
    state: &mut StackConfigEditorState,
    key: KeyEvent,
    known_stacks: &[String],
) -> WizardOutcome {
    match &mut state.step {
        StackConfigEditorStep::Overview => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Up => {
                state.selected_field = state.selected_field.saturating_sub(1);
                state.error = None;
            }
            KeyCode::Down => {
                state.selected_field = (state.selected_field + 1).min(11);
                state.error = None;
            }
            KeyCode::Left | KeyCode::Char('-') => {
                state.error = None;
                match state.selected_field {
                    0 => {
                        state.deploy_enabled = false;
                        state.activated_at = None;
                    }
                    1 => state.cpu_cores = state.cpu_cores.saturating_sub(1).max(1),
                    2 => state.memory_mb = state.memory_mb.saturating_sub(512).max(512),
                    3 => state.disk_gb = state.disk_gb.saturating_sub(1).max(8),
                    6 => state.ip_mode = previous_ip_mode(&state.ip_mode),
                    8 => state.autostart = false,
                    9 => state.startup_order = state.startup_order.saturating_sub(5).max(5),
                    _ => {}
                }
            }
            KeyCode::Right | KeyCode::Char('+') => {
                state.error = None;
                match state.selected_field {
                    0 => state.deploy_enabled = true,
                    1 => state.cpu_cores = state.cpu_cores.saturating_add(1).min(8),
                    2 => state.memory_mb = state.memory_mb.saturating_add(512).min(65536),
                    3 => state.disk_gb = state.disk_gb.saturating_add(1).min(2048),
                    6 => state.ip_mode = next_ip_mode(&state.ip_mode),
                    8 => state.autostart = true,
                    9 => state.startup_order = state.startup_order.saturating_add(5).min(500),
                    _ => {}
                }
            }
            KeyCode::Enter => match state.selected_field {
                4 => {
                    let mut input = Input::default();
                    input = input.with_value(state.hostname.clone());
                    state.step = StackConfigEditorStep::EditHostname { input };
                }
                5 => {
                    let mut input = Input::default();
                    input = input.with_value(state.hwaddr.clone());
                    state.step = StackConfigEditorStep::EditHwaddr { input };
                }
                7 => {
                    let mut input = Input::default();
                    input = input.with_value(state.reserved_ipv4.clone().unwrap_or_default());
                    state.step = StackConfigEditorStep::EditReservedIpv4 { input };
                }
                8 => {
                    state.autostart = !state.autostart;
                }
                9 => {
                    state.startup_order = state.startup_order.saturating_add(5).min(500);
                }
                10 => {
                    let config = crate::scaffold::StackConfig {
                        stack_name: state.stack_name.clone(),
                        vmid: state.vmid,
                        hostname: state.hostname.clone(),
                        hwaddr: state.hwaddr.clone(),
                        deploy_enabled: state.deploy_enabled,
                        activated_at: state.activated_at.clone(),
                        bridge: state.bridge.clone(),
                        ip_mode: state.ip_mode.clone(),
                        reserved_ipv4: state.reserved_ipv4.clone(),
                        autostart: state.autostart,
                        startup_order: state.startup_order,
                        cpu_cores: state.cpu_cores,
                        memory_mb: state.memory_mb,
                        disk_gb: state.disk_gb,
                        host_storage_path: format!("/opt/appdata/{}", state.stack_name),
                        mount_point: "/appdata".to_string(),
                        lxc_template: "debian-12-standard 12.12-1 amd64".to_string(),
                        unprivileged: true,
                        features: vec!["nesting=1".to_string()],
                        tun_device: None,
                    };

                    match crate::opnsense::ensure_stack_reservation(&config, known_stacks) {
                        Ok(outcome) => {
                            state.error = Some(format!(
                                "DHCP reservation {} for {} (deleted {} old stack-owned conflict{}).",
                                if outcome.updated {
                                    "updated"
                                } else {
                                    "created"
                                },
                                outcome.reserved_ipv4,
                                outcome.deleted_conflicts,
                                if outcome.deleted_conflicts == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            ));
                        }
                        Err(e) => {
                            state.error = Some(format!("DHCP sync failed: {}", e));
                        }
                    }
                }
                11 => {
                    let activated_at = if state.deploy_enabled {
                        state
                            .activated_at
                            .clone()
                            .or_else(|| Some(current_epoch_string()))
                    } else {
                        None
                    };

                    let config = crate::scaffold::StackConfig {
                        stack_name: state.stack_name.clone(),
                        vmid: state.vmid,
                        hostname: state.hostname.clone(),
                        hwaddr: state.hwaddr.clone(),
                        deploy_enabled: state.deploy_enabled,
                        activated_at,
                        bridge: state.bridge.clone(),
                        ip_mode: state.ip_mode.clone(),
                        reserved_ipv4: state.reserved_ipv4.clone(),
                        autostart: state.autostart,
                        startup_order: state.startup_order,
                        cpu_cores: state.cpu_cores,
                        memory_mb: state.memory_mb,
                        disk_gb: state.disk_gb,
                        host_storage_path: format!("/opt/appdata/{}", state.stack_name),
                        mount_point: "/appdata".to_string(),
                        lxc_template: "debian-12-standard 12.12-1 amd64".to_string(),
                        unprivileged: true,
                        features: vec!["nesting=1".to_string()],
                        tun_device: None,
                    };

                    match crate::scaffold::save_stack_config(&config) {
                        Ok(()) => {
                            let _ = crate::gitops::commit_and_push(
                                ".",
                                &format!("Update stack config for {}", state.stack_name),
                            );
                            state.activated_at = config.activated_at.clone();
                            state.error = None;
                            state.step = StackConfigEditorStep::Done {
                                summary: format!(
                                    "Saved lxc-compose settings for '{}'.\nDeploy enabled: {}\nResources: {} cores / {:.1} GiB / {} GiB\nHostname: {}\nIP mode: {}\nReserved IPv4: {}\nAutostart: {}\nBoot order: {}",
                                    state.stack_name,
                                    state.deploy_enabled,
                                    state.cpu_cores,
                                    state.memory_mb as f32 / 1024.0,
                                    state.disk_gb,
                                    state.hostname,
                                    state.ip_mode,
                                    state.reserved_ipv4.as_deref().unwrap_or("(unset)"),
                                    state.autostart,
                                    state.startup_order,
                                ),
                            };
                        }
                        Err(e) => {
                            state.error = Some(format!("save failed: {}", e));
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        },
        StackConfigEditorStep::EditHostname { input } => {
            if key.code == KeyCode::Esc {
                state.error = None;
                state.step = StackConfigEditorStep::Overview;
            } else if key.code == KeyCode::Enter {
                let hostname = input.value().trim();
                if hostname.is_empty() || hostname.contains(' ') {
                    state.error =
                        Some("hostname must be non-empty and contain no spaces".to_string());
                } else {
                    state.hostname = hostname.to_string();
                    state.error = None;
                    state.step = StackConfigEditorStep::Overview;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }
        StackConfigEditorStep::EditHwaddr { input } => {
            if key.code == KeyCode::Esc {
                state.error = None;
                state.step = StackConfigEditorStep::Overview;
            } else if key.code == KeyCode::Enter {
                let hwaddr = input.value().trim().to_lowercase();
                if is_valid_mac_address(&hwaddr) {
                    state.hwaddr = hwaddr;
                    state.error = None;
                    state.step = StackConfigEditorStep::Overview;
                } else {
                    state.error = Some("hwaddr must be six hex pairs separated by ':'".to_string());
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }
        StackConfigEditorStep::EditReservedIpv4 { input } => {
            if key.code == KeyCode::Esc {
                state.error = None;
                state.step = StackConfigEditorStep::Overview;
            } else if key.code == KeyCode::Enter {
                let value = input.value().trim();
                if value.is_empty() {
                    state.reserved_ipv4 = None;
                    state.error = None;
                    state.step = StackConfigEditorStep::Overview;
                } else if is_valid_ipv4(value) {
                    state.reserved_ipv4 = Some(value.to_string());
                    state.error = None;
                    state.step = StackConfigEditorStep::Overview;
                } else {
                    state.error = Some("reserved IPv4 must be a valid IPv4 address".to_string());
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }
        StackConfigEditorStep::Done { .. } => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return WizardOutcome::Close;
            }
        }
    }

    WizardOutcome::Continue
}

fn handle_app_config_editor(state: &mut AppConfigEditorState, key: KeyEvent) -> WizardOutcome {
    match &mut state.step {
        AppConfigEditorStep::Overview => match key.code {
            KeyCode::Esc => return WizardOutcome::Close,
            KeyCode::Up => {
                state.selected_field = state.selected_field.saturating_sub(1);
                state.error = None;
            }
            KeyCode::Down => {
                state.selected_field = (state.selected_field + 1).min(1);
                state.error = None;
            }
            KeyCode::Enter => match state.selected_field {
                0 => {
                    let mut input = Input::default();
                    input = input.with_value(state.docker_image.clone());
                    state.step = AppConfigEditorStep::EditDockerImage { input };
                }
                1 => {
                    if state.docker_image.trim().is_empty() {
                        state.error = Some("docker image must be non-empty".to_string());
                    } else {
                        match crate::stack_features::set_app_docker_image(
                            &state.stack_name,
                            &state.app_name,
                            state.docker_image.trim(),
                        ) {
                            Ok(()) => {
                                let _ = crate::gitops::commit_and_push(
                                    ".",
                                    &format!(
                                        "Update docker image for {}/{}",
                                        state.stack_name, state.app_name
                                    ),
                                );
                                state.step = AppConfigEditorStep::Done {
                                    summary: format!(
                                        "Saved compose config for '{}/{}'.\nDocker image: {}",
                                        state.stack_name, state.app_name, state.docker_image
                                    ),
                                };
                            }
                            Err(e) => {
                                state.error = Some(format!("save failed: {}", e));
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        },
        AppConfigEditorStep::EditDockerImage { input } => {
            if key.code == KeyCode::Esc {
                state.error = None;
                state.step = AppConfigEditorStep::Overview;
            } else if key.code == KeyCode::Enter {
                let image = input.value().trim();
                if image.is_empty() {
                    state.error = Some("docker image must be non-empty".to_string());
                } else {
                    state.docker_image = image.to_string();
                    state.error = None;
                    state.step = AppConfigEditorStep::Overview;
                }
            } else {
                input.handle_event(&crossterm::event::Event::Key(key));
            }
        }
        AppConfigEditorStep::Done { .. } => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                return WizardOutcome::Close;
            }
        }
    }

    WizardOutcome::Continue
}

fn current_epoch_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn next_ip_mode(current: &str) -> String {
    match current {
        "manual" => "dhcp-reserved".to_string(),
        _ => "manual".to_string(),
    }
}

fn previous_ip_mode(current: &str) -> String {
    next_ip_mode(current)
}

fn is_valid_mac_address(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn is_valid_ipv4(value: &str) -> bool {
    value.parse::<std::net::Ipv4Addr>().is_ok()
}

// ── Normal navigation (no modal open) ────────────────────────────────────────

fn handle_navigation(app: &mut App, key: KeyEvent) -> EventOutcome {
    match app.active_tab() {
        Tab::Scaffolding => handle_scaffolding_nav(app, key),
        Tab::Backups => handle_backups_nav(app, key),
        Tab::Logs => handle_logs_nav(app, key),
        Tab::HostManagement => handle_host_management_nav(app, key),
        _ => handle_generic_nav(app, key),
    }
}

fn handle_backups_nav(app: &mut App, key: KeyEvent) -> EventOutcome {
    match key.code {
        KeyCode::Char('e') => {
            app.backup_schedule.enabled = !app.backup_schedule.enabled;
            app.backup_status = format!("enabled set to {}", app.backup_schedule.enabled);
        }
        KeyCode::Char('+') => {
            app.backup_schedule.interval_minutes =
                app.backup_schedule.interval_minutes.saturating_add(15);
            app.backup_status = format!(
                "interval set to {} minutes",
                app.backup_schedule.interval_minutes
            );
        }
        KeyCode::Char('-') => {
            app.backup_schedule.interval_minutes = app
                .backup_schedule
                .interval_minutes
                .saturating_sub(15)
                .max(15);
            app.backup_status = format!(
                "interval set to {} minutes",
                app.backup_schedule.interval_minutes
            );
        }
        KeyCode::Char('d') => {
            app.backup_schedule.retention_daily =
                app.backup_schedule.retention_daily.saturating_add(1);
            app.backup_status = format!("daily retention: {}", app.backup_schedule.retention_daily);
        }
        KeyCode::Char('D') => {
            app.backup_schedule.retention_daily =
                app.backup_schedule.retention_daily.saturating_sub(1).max(1);
            app.backup_status = format!("daily retention: {}", app.backup_schedule.retention_daily);
        }
        KeyCode::Char('w') => {
            app.backup_schedule.retention_weekly =
                app.backup_schedule.retention_weekly.saturating_add(1);
            app.backup_status =
                format!("weekly retention: {}", app.backup_schedule.retention_weekly);
        }
        KeyCode::Char('W') => {
            app.backup_schedule.retention_weekly = app
                .backup_schedule
                .retention_weekly
                .saturating_sub(1)
                .max(1);
            app.backup_status =
                format!("weekly retention: {}", app.backup_schedule.retention_weekly);
        }
        KeyCode::Char('m') => {
            app.backup_schedule.retention_monthly =
                app.backup_schedule.retention_monthly.saturating_add(1);
            app.backup_status = format!(
                "monthly retention: {}",
                app.backup_schedule.retention_monthly
            );
        }
        KeyCode::Char('M') => {
            app.backup_schedule.retention_monthly = app
                .backup_schedule
                .retention_monthly
                .saturating_sub(1)
                .max(1);
            app.backup_status = format!(
                "monthly retention: {}",
                app.backup_schedule.retention_monthly
            );
        }
        KeyCode::Char('n') => {
            app.backup_schedule.notify_on_success = !app.backup_schedule.notify_on_success;
            app.backup_status = format!(
                "notify_on_success set to {}",
                app.backup_schedule.notify_on_success
            );
        }
        KeyCode::Char('f') => {
            app.backup_schedule.notify_on_failure = !app.backup_schedule.notify_on_failure;
            app.backup_status = format!(
                "notify_on_failure set to {}",
                app.backup_schedule.notify_on_failure
            );
        }
        KeyCode::Char('s') => match app.backup_schedule.save() {
            Ok(()) => {
                app.backup_status = "backup policy saved".to_string();
            }
            Err(e) => {
                app.backup_status = format!("save failed: {}", e);
            }
        },
        KeyCode::Char('b') => {
            let entries: Vec<OperationEntry> = app
                .stacks
                .iter()
                .map(|s| OperationEntry {
                    name: s.clone(),
                    status: "✓".to_string(),
                    detail: "backup queued".to_string(),
                })
                .collect();

            app.modal = ActiveModal::OperationProgress(OperationProgressState {
                title: "Manual Backup - All Stacks".to_string(),
                phase: "HOST backup orchestration".to_string(),
                summary: format!("Queued {} stack backup scope entries.", entries.len()),
                entries,
            });
            app.backup_status = "manual backup-all queued".to_string();
        }
        KeyCode::Char('i') => {
            let stack = app
                .stacks
                .get(app.selected_stack)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            app.modal = ActiveModal::OperationProgress(OperationProgressState {
                title: format!("Individual Restore - {}", stack),
                phase: "Restore dispatch".to_string(),
                summary: format!(
                    "Dispatching restore for '{}' with backup_id='{}'.",
                    stack, app.restore_backup_id
                ),
                entries: vec![OperationEntry {
                    name: stack.clone(),
                    status: "⟳".to_string(),
                    detail: "queued for /api/restore".to_string(),
                }],
            });
            app.restore_stack = stack.clone();
            app.restore_pending = true;
            app.backup_status = format!(
                "individual restore queued for '{}' using backup '{}'",
                stack, app.restore_backup_id
            );
        }
        KeyCode::Char('r') => {
            let entries: Vec<OperationEntry> = app
                .stacks
                .iter()
                .map(|s| OperationEntry {
                    name: s.clone(),
                    status: "⟳".to_string(),
                    detail: "queued for /api/restore".to_string(),
                })
                .collect();
            app.modal = ActiveModal::OperationProgress(OperationProgressState {
                title: "Full Backup Restore".to_string(),
                phase: "Restore dispatch".to_string(),
                summary: format!(
                    "Queueing restore dispatch for {} stack(s) with backup_id='{}'.",
                    entries.len(),
                    app.restore_backup_id
                ),
                entries,
            });
            app.restore_queue = app.stacks.iter().cloned().collect();
            app.backup_status = format!(
                "full restore queued for {} stack(s) using backup '{}'",
                app.restore_queue.len(),
                app.restore_backup_id
            );
        }
        KeyCode::Char('p') => {
            let entries: Vec<OperationEntry> = app
                .stacks
                .iter()
                .map(|s| OperationEntry {
                    name: s.clone(),
                    status: "⟳".to_string(),
                    detail: "os patch queued".to_string(),
                })
                .collect();
            app.modal = ActiveModal::OperationProgress(OperationProgressState {
                title: "OS Patching".to_string(),
                phase: "LXC patch orchestration".to_string(),
                summary: format!("Patch-all prepared for {} stack(s).", entries.len()),
                entries,
            });
            app.backup_status = "os patch all prepared".to_string();
        }
        KeyCode::Char('u') => {
            let entries: Vec<OperationEntry> = app
                .stacks
                .iter()
                .map(|s| OperationEntry {
                    name: s.clone(),
                    status: "✓".to_string(),
                    detail: "unattended-upgrades policy: active".to_string(),
                })
                .collect();
            app.modal = ActiveModal::OperationProgress(OperationProgressState {
                title: "Unattended Upgrades".to_string(),
                phase: "Policy status view".to_string(),
                summary: format!("OS auto-patch status across {} stack(s).", entries.len()),
                entries,
            });
            app.backup_status = "unattended-upgrades status refreshed".to_string();
        }
        KeyCode::Tab => app.tab_right(),
        KeyCode::BackTab => app.tab_left(),
        KeyCode::Char('q') => return EventOutcome::Quit,
        _ => {}
    }

    EventOutcome::Continue
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
        KeyCode::Up => {
            app.host_selected = app.host_selected.saturating_sub(1);
            EventOutcome::Continue
        }
        KeyCode::Down => {
            if app.host_selected + 1 < app.stacks.len() {
                app.host_selected += 1;
            }
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
    if key.code == KeyCode::Char('n') {
        app.modal = ActiveModal::StackCreationWizard(StackCreationWizardState {
            stack_name: None,
            cpu_cores: 2,
            memory_mb: 2048,
            disk_gb: 32,
            autostart: true,
            startup_order: 90,
            step: StackCreationStep::Name {
                input: Input::default(),
                error: None,
            },
        });
        return EventOutcome::Continue;
    }

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

        // Marks the selected stack as deploy-enabled in lxc-compose.yml.
        // Deployment (`s`) only runs for activated stacks.
        KeyCode::Char('a') => {
            if let Some(stack_name) = app.stacks.get(app.selected_stack) {
                match crate::scaffold::ensure_lxc_compose(stack_name)
                    .and_then(|_| crate::scaffold::set_stack_deploy_enabled(stack_name, true))
                {
                    Ok(()) => {
                        app.sync_status = format!(
                            "Activated '{}' (deploy.enabled=true). Press 's' to deploy.",
                            stack_name
                        );
                        app.push_log(
                            "CLIENT",
                            "INFO",
                            &format!("Stack '{}' activated in lxc-compose.yml", stack_name),
                        );
                    }
                    Err(e) => {
                        app.sync_status = format!("Activation failed for '{}': {}", stack_name, e);
                        app.push_log(
                            "CLIENT",
                            "ERROR",
                            &format!("Activation failed for '{}': {}", stack_name, e),
                        );
                    }
                }
            }
            return EventOutcome::Continue;
        }

        // Marks selected stack as inactive for deploy command.
        KeyCode::Char('x') => {
            if let Some(stack_name) = app.stacks.get(app.selected_stack) {
                match crate::scaffold::ensure_lxc_compose(stack_name)
                    .and_then(|_| crate::scaffold::set_stack_deploy_enabled(stack_name, false))
                {
                    Ok(()) => {
                        app.sync_status =
                            format!("Deactivated '{}' (deploy.enabled=false).", stack_name);
                        app.push_log(
                            "CLIENT",
                            "INFO",
                            &format!("Stack '{}' deactivated in lxc-compose.yml", stack_name),
                        );
                    }
                    Err(e) => {
                        app.sync_status =
                            format!("Deactivation failed for '{}': {}", stack_name, e);
                        app.push_log(
                            "CLIENT",
                            "ERROR",
                            &format!("Deactivation failed for '{}': {}", stack_name, e),
                        );
                    }
                }
            }
            return EventOutcome::Continue;
        }

        // Add missing core apps to selected stack.
        KeyCode::Char('c') => {
            if let Some(stack_name) = app.stacks.get(app.selected_stack) {
                match crate::stack_features::add_missing_core_apps(stack_name) {
                    Ok(result) if result.added.is_empty() => {
                        app.sync_status = format!("No missing core apps in '{}'.", stack_name);
                    }
                    Ok(result) => {
                        let added = result.added.join(", ");
                        let _ = crate::gitops::commit_and_push(
                            ".",
                            &format!(
                                "feat(scaffold): add core apps [{}] to stack {}",
                                added, stack_name
                            ),
                        );
                        app.sync_status = format!("Added core apps to '{}': {}", stack_name, added);

                        if crate::scaffold::is_stack_deploy_enabled(stack_name).unwrap_or(false) {
                            app.sync_stack = stack_name.clone();
                            app.sync_pending = true;
                        }
                    }
                    Err(e) => {
                        app.sync_status = format!("Core-app scaffolding failed: {}", e);
                    }
                }
            }
            return EventOutcome::Reload;
        }

        // Trigger a GitOps sync on the LXC that manages the selected stack.
        // The main loop picks up `sync_pending` and sends the HTTP request.
        KeyCode::Char('s') => {
            if let Some(stack_name) = app.stacks.get(app.selected_stack).cloned() {
                if let Err(e) = crate::stack_features::validate_setup_hook(&stack_name) {
                    app.sync_status = format!("Deploy blocked for '{}': {}", stack_name, e);
                    app.push_client_logfmt(
                        "ERROR",
                        Some(&stack_name),
                        Some("pre_sync_validation"),
                        "fail-closed deploy abort",
                        Some(&e.to_string()),
                    );
                    return EventOutcome::Continue;
                }

                if let Err(e) = crate::stack_features::validate_stack_filesystem_layout(&stack_name)
                {
                    app.sync_status = format!("Deploy blocked for '{}': {}", stack_name, e);
                    app.push_client_logfmt(
                        "ERROR",
                        Some(&stack_name),
                        Some("filesystem_layout"),
                        "fail-closed deploy abort",
                        Some(&e.to_string()),
                    );
                    return EventOutcome::Continue;
                }

                let activation_state = crate::scaffold::ensure_lxc_compose(&stack_name)
                    .and_then(|_| crate::scaffold::is_stack_deploy_enabled(&stack_name));

                match activation_state {
                    Ok(true) => {
                        app.sync_stack = stack_name.clone();
                        app.sync_pending = true;
                        app.sync_status = format!("Queued sync for '{}'…", stack_name);
                    }
                    Ok(false) => {
                        app.sync_status = format!(
                            "Stack '{}' is inactive. Press 'a' to activate first.",
                            stack_name
                        );
                        app.push_client_logfmt(
                            "WARN",
                            Some(&stack_name),
                            Some("deploy_gate"),
                            "deploy blocked: deploy.enabled is false",
                            None,
                        );
                    }
                    Err(e) => {
                        app.sync_status =
                            format!("Cannot read activation state for '{}': {}", stack_name, e);
                        app.push_client_logfmt(
                            "ERROR",
                            Some(&stack_name),
                            Some("deploy_gate"),
                            "deploy activation check failed",
                            Some(&e.to_string()),
                        );
                    }
                }
            }
            return EventOutcome::Continue;
        }

        // Queue sync for all stacks that are currently deploy-enabled.
        KeyCode::Char('D') | KeyCode::Char('u') => {
            let active = crate::scaffold::list_deploy_enabled_stacks(&app.stacks);
            if active.is_empty() {
                app.sync_status = "No active stacks to deploy/update.".to_string();
                return EventOutcome::Continue;
            }

            for stack in active {
                if let Err(e) = crate::stack_features::validate_setup_hook(&stack) {
                    app.push_client_logfmt(
                        "ERROR",
                        Some(&stack),
                        Some("pre_sync_validation"),
                        "fail-closed batch skip",
                        Some(&e.to_string()),
                    );
                    continue;
                }

                if let Err(e) = crate::stack_features::validate_stack_filesystem_layout(&stack) {
                    app.push_client_logfmt(
                        "ERROR",
                        Some(&stack),
                        Some("filesystem_layout"),
                        "fail-closed batch skip",
                        Some(&e.to_string()),
                    );
                    continue;
                }

                if !app.sync_queue.iter().any(|queued| queued == &stack)
                    && !(app.sync_pending && app.sync_stack == stack)
                {
                    app.sync_queue.push_back(stack);
                }
            }

            let queued_count = app.sync_queue.len();
            app.sync_status = if queued_count == 0 {
                "No stacks queued (all failed pre-sync validation).".to_string()
            } else {
                format!("Queued {} active stack sync job(s).", queued_count)
            };
            return EventOutcome::Continue;
        }

        // Enable GPU passthrough wiring for selected app and mark host hint in lxc-compose.
        KeyCode::Char('g') => {
            let stack_name = app.stacks.get(app.selected_stack).cloned();
            let app_name = selected_scaffolding_app_name(app);

            if let (Some(stack_name), Some(app_name)) = (stack_name, app_name) {
                match crate::stack_features::set_gpu_passthrough_for_app(
                    &stack_name,
                    &app_name,
                    true,
                ) {
                    Ok(()) => {
                        let _ = crate::gitops::commit_and_push(
                            ".",
                            &format!(
                                "feat(hardware): enable gpu passthrough for {} in {}",
                                app_name, stack_name
                            ),
                        );

                        app.sync_status =
                            format!("GPU passthrough enabled for '{}/{}'.", stack_name, app_name);
                        app.push_client_logfmt(
                            "INFO",
                            Some(&stack_name),
                            Some("gpu_passthrough"),
                            &format!("enabled target_app={}", app_name),
                            None,
                        );

                        if crate::scaffold::is_stack_deploy_enabled(&stack_name).unwrap_or(false) {
                            app.sync_stack = stack_name;
                            app.sync_pending = true;
                        }
                        return EventOutcome::Reload;
                    }
                    Err(e) => {
                        app.sync_status = format!("GPU enable failed: {}", e);
                    }
                }
            } else {
                app.sync_status = "Select an app row first before enabling GPU.".to_string();
            }

            return EventOutcome::Continue;
        }

        // Disable GPU passthrough wiring for selected app and clear host hint.
        KeyCode::Char('G') => {
            let stack_name = app.stacks.get(app.selected_stack).cloned();
            let app_name = selected_scaffolding_app_name(app);

            if let (Some(stack_name), Some(app_name)) = (stack_name, app_name) {
                match crate::stack_features::set_gpu_passthrough_for_app(
                    &stack_name,
                    &app_name,
                    false,
                ) {
                    Ok(()) => {
                        let _ = crate::gitops::commit_and_push(
                            ".",
                            &format!(
                                "feat(hardware): disable gpu passthrough for {} in {}",
                                app_name, stack_name
                            ),
                        );

                        app.sync_status = format!(
                            "GPU passthrough disabled for '{}/{}'.",
                            stack_name, app_name
                        );
                        app.push_client_logfmt(
                            "INFO",
                            Some(&stack_name),
                            Some("gpu_passthrough"),
                            &format!("disabled target_app={}", app_name),
                            None,
                        );

                        if crate::scaffold::is_stack_deploy_enabled(&stack_name).unwrap_or(false) {
                            app.sync_stack = stack_name;
                            app.sync_pending = true;
                        }
                        return EventOutcome::Reload;
                    }
                    Err(e) => {
                        app.sync_status = format!("GPU disable failed: {}", e);
                    }
                }
            } else {
                app.sync_status = "Select an app row first before disabling GPU.".to_string();
            }

            return EventOutcome::Continue;
        }

        _ => {}
    }

    EventOutcome::Continue
}

fn selected_scaffolding_app_name(app: &App) -> Option<String> {
    if app.stacks.is_empty() {
        return None;
    }

    let dropdown = app.stack_dropdowns.get(app.selected_stack)?;
    let idx = dropdown.selected_option;
    let mut cursor = 2usize;

    for (i, name) in dropdown.apps.iter().enumerate() {
        if idx == cursor {
            return Some(name.clone());
        }
        cursor += 1;
        if dropdown
            .app_dropdowns
            .get(i)
            .map(|d| d.expanded)
            .unwrap_or(false)
        {
            cursor += 2;
        }
    }

    None
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
                        selected_defaults: Vec::new(),
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
                2 => match crate::scaffold::read_stack_config(&stack_name) {
                    Ok(config) => {
                        let pre_sync_exists =
                            std::path::Path::new(&format!("stacks/{}/setup.sh", stack_name))
                                .exists();
                        app.modal = ActiveModal::StackConfigEditor(StackConfigEditorState {
                            stack_name: config.stack_name,
                            vmid: config.vmid,
                            hostname: config.hostname,
                            hwaddr: config.hwaddr,
                            bridge: config.bridge,
                            ip_mode: config.ip_mode,
                            reserved_ipv4: config.reserved_ipv4,
                            autostart: config.autostart,
                            startup_order: config.startup_order,
                            cpu_cores: config.cpu_cores,
                            memory_mb: config.memory_mb,
                            disk_gb: config.disk_gb,
                            deploy_enabled: config.deploy_enabled,
                            activated_at: config.activated_at,
                            pre_sync_exists,
                            selected_field: 0,
                            error: None,
                            step: StackConfigEditorStep::Overview,
                        });
                    }
                    Err(e) => {
                        app.sync_status =
                            format!("Cannot open stack config for '{}': {}", stack_name, e);
                    }
                },
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
                        match crate::stack_features::read_app_docker_image(&stack_name, &d.apps[i])
                        {
                            Ok(docker_image) => {
                                app.modal = ActiveModal::AppConfigEditor(AppConfigEditorState {
                                    stack_name: stack_name.clone(),
                                    app_name: d.apps[i].clone(),
                                    docker_image,
                                    selected_field: 0,
                                    error: None,
                                    step: AppConfigEditorStep::Overview,
                                });
                            }
                            Err(e) => {
                                app.sync_status = format!(
                                    "Cannot open app config for '{}/{}': {}",
                                    stack_name, d.apps[i], e
                                );
                            }
                        }
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

        SshAddStep::Ip {
            alias,
            input,
            error,
        } => {
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
