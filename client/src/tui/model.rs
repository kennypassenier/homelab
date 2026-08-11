//! Elm-style model + messages (AR6). `update` is pure over (Model, Msg) and
//! returns any Command to send; `view` (in view/) only reads the model.

use std::collections::VecDeque;

use homelab_proto::{Command, FleetState, LogLevel, ServerMsg};

use crate::tui::backend::BackendEvent;
use crate::tui::fx::FxLevel;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Splash,
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Stacks,
    Logs,
    Doctor,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Dashboard, Tab::Stacks, Tab::Logs, Tab::Doctor];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "DASHBOARD",
            Tab::Stacks => "STACKS",
            Tab::Logs => "LOG_STREAM",
            Tab::Doctor => "DOCTOR",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Conn {
    Connecting,
    Up,
    Down,
}

pub struct LogRow {
    pub level: LogLevel,
    pub source: String,
    pub msg: String,
}

/// A live transfer for the G6 visual.
pub struct Transfer {
    pub label: String,
    pub done: u64,
    pub total: Option<u64>,
    pub age: u16, // ticks since last update, for fade-out
}

pub enum Msg {
    Key(crossterm::event::KeyEvent),
    Tick,
    Backend(BackendEvent),
}

/// Focus mode (mockup-approved): a near-fullscreen task window showing only
/// this operation's feed while it runs.
pub struct Focus {
    pub title: String,
    pub feed: Vec<LogRow>,
    pub scroll: usize,
    pub done: bool,
    pub ok: bool,
    pub result: String,
}

/// D6 change-plan preview: what a deploy would do, shown before it runs.
/// ENTER executes, ESC cancels.
pub struct Plan {
    pub stack: String,
    pub lines: Vec<(char, String)>, // sign +/-/~/space, text
    pub spec: Box<homelab_proto::DeploySpec>,
}

/// G2 new-stack wizard.
pub struct Preset {
    pub name: &'static str,
    pub desc: &'static str,
    pub app: Option<(&'static str, &'static str)>, // (app, image)
    pub ram: u32,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "syncthing",
        desc: "Obsidian vault peer",
        app: Some(("syncthing", "syncthing/syncthing:latest")),
        ram: 512,
    },
    Preset {
        name: "jellyfin",
        desc: "Media server (VAAPI)",
        app: Some(("jellyfin", "jellyfin/jellyfin:latest")),
        ram: 4096,
    },
    Preset {
        name: "mealie",
        desc: "Recipes + meal planning",
        app: Some(("mealie", "ghcr.io/mealie-recipes/mealie:latest")),
        ram: 512,
    },
    Preset {
        name: "actual",
        desc: "Envelope budgeting",
        app: Some(("actual", "actualbudget/actual-server:latest")),
        ram: 512,
    },
    Preset {
        name: "uptime-kuma",
        desc: "Uptime monitoring",
        app: Some(("uptime-kuma", "louislam/uptime-kuma:1")),
        ram: 512,
    },
    Preset {
        name: "custom",
        desc: "Empty stack — add apps later",
        app: None,
        ram: 1024,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WizStep {
    Preset,
    Name,
    Review,
}

pub struct Wizard {
    pub step: WizStep,
    pub preset_idx: usize,
    pub name: String,
}

pub struct Model {
    pub screen: Screen,
    pub tab: Tab,
    pub fx: FxLevel,
    pub tick: u64,
    pub reveal_start: u64,
    pub flicker: u8,
    pub conn: Conn,
    pub host_version: String,
    pub fingerprint: Option<String>,
    pub status_line: String,

    pub fleet: Option<FleetState>,
    pub selected_stack: usize,
    pub logs: VecDeque<LogRow>,
    pub log_scroll: usize,
    pub log_follow: bool,
    pub log_source: usize, // 0 = ALL, else stack index + 1
    pub transfers: Vec<Transfer>,
    pub palette_open: bool,
    pub palette_input: String,
    pub palette_sel: usize,
    pub help_open: bool,
    pub doctor_text: Vec<String>,
    /// Deployable stack dirs found locally (name, path).
    pub local_stacks: Vec<(String, std::path::PathBuf)>,
    pub focus: Option<Focus>,
    pub plan: Option<Plan>,
    pub wizard: Option<Wizard>,

    pub should_quit: bool,
    /// Commands the update fn wants sent to the backend this cycle.
    pub outbox: Vec<Command>,
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    pub fn new() -> Self {
        Self {
            screen: Screen::Splash,
            tab: Tab::Dashboard,
            fx: FxLevel::Full,
            tick: 0,
            reveal_start: 0,
            flicker: 0,
            conn: Conn::Connecting,
            host_version: String::new(),
            fingerprint: None,
            status_line: "establishing link…".into(),
            fleet: None,
            selected_stack: 0,
            logs: VecDeque::with_capacity(600),
            log_scroll: 0,
            log_follow: true,
            log_source: 0,
            transfers: Vec::new(),
            palette_open: false,
            palette_input: String::new(),
            palette_sel: 0,
            help_open: false,
            doctor_text: Vec::new(),
            local_stacks: Vec::new(),
            focus: None,
            plan: None,
            wizard: None,
            should_quit: false,
            outbox: Vec::new(),
        }
    }

    pub fn reveal_progress(&self) -> f32 {
        if self.fx == FxLevel::Off {
            return 1.0; // no reveal animation when effects are off
        }
        ((self.tick.saturating_sub(self.reveal_start)) as f32 / 9.0).min(1.0)
    }

    pub fn stack_count(&self) -> usize {
        self.fleet.as_ref().map(|f| f.stacks.len()).unwrap_or(0)
    }

    fn switch_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.reveal_start = self.tick;
            self.flicker = 4;
            if tab == Tab::Doctor {
                self.outbox.push(Command::Doctor);
            }
        }
    }

    fn push_log(&mut self, level: LogLevel, source: String, msg: String) {
        if self.logs.len() > 500 {
            self.logs.pop_front();
        }
        self.logs.push_back(LogRow { level, source, msg });
        if !self.log_follow {
            self.log_scroll += 1;
        }
    }
}

/// The pure update. Mutates the model and queues commands in `model.outbox`.
pub fn update(model: &mut Model, msg: Msg) {
    match msg {
        Msg::Tick => {
            model.tick += 1;
            if model.flicker > 0 {
                model.flicker -= 1;
            }
            for t in &mut model.transfers {
                t.age = t.age.saturating_add(1);
            }
            model.transfers.retain(|t| t.age < 30);
            if model.screen == Screen::Splash && model.tick > 120 {
                enter_main(model);
            }
        }
        Msg::Backend(ev) => on_backend(model, ev),
        Msg::Key(key) => on_key(model, key),
    }
}

fn enter_main(model: &mut Model) {
    model.screen = Screen::Main;
    model.reveal_start = model.tick;
    model.flicker = 4;
    model.outbox.push(Command::GetState);
}

fn on_backend(model: &mut Model, ev: BackendEvent) {
    match ev {
        BackendEvent::Connected {
            version,
            fingerprint,
        } => {
            model.conn = Conn::Up;
            model.host_version = version;
            model.fingerprint = fingerprint;
            model.status_line = "link established".into();
            model.outbox.push(Command::GetState);
        }
        BackendEvent::Disconnected(why) => {
            model.conn = Conn::Down;
            model.status_line = format!("link down: {}", why);
        }
        BackendEvent::Server(sm) => match sm {
            ServerMsg::Hello { version, .. } => {
                model.host_version = version;
                model.conn = Conn::Up;
            }
            ServerMsg::Log { level, source, msg } => {
                if let Some(focus) = model.focus.as_mut() {
                    if !focus.done {
                        focus.feed.push(LogRow {
                            level,
                            source: source.clone(),
                            msg: msg.clone(),
                        });
                    }
                }
                model.push_log(level, source, msg);
            }
            ServerMsg::Transfer {
                label, done, total, ..
            } => {
                if let Some(t) = model.transfers.iter_mut().find(|t| t.label == label) {
                    t.done = done;
                    t.total = total;
                    t.age = 0;
                } else {
                    model.transfers.push(Transfer {
                        label,
                        done,
                        total,
                        age: 0,
                    });
                }
            }
            ServerMsg::State(fleet) => {
                model.fleet = Some(*fleet);
                let n = model.stack_count();
                if n > 0 && model.selected_stack >= n {
                    model.selected_stack = n - 1;
                }
            }
            ServerMsg::RpcDone(resp) => {
                if let Some(focus) = model.focus.as_mut() {
                    if !focus.done {
                        focus.done = true;
                        focus.ok = resp.ok;
                        focus.result = resp.message.clone();
                        model.status_line = if resp.ok {
                            "deploy complete".into()
                        } else {
                            "deploy FAILED — see focus feed".into()
                        };
                        return;
                    }
                }
                if model.tab == Tab::Doctor {
                    model.doctor_text = resp.message.lines().map(|s| s.to_string()).collect();
                }
                model.status_line = resp.message.lines().next().unwrap_or("").to_string();
            }
        },
    }
}

fn on_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::{KeyCode, KeyModifiers};

    if model.screen == Screen::Splash {
        enter_main(model);
        return;
    }
    if model.palette_open {
        palette_key(model, key);
        return;
    }
    if let Some(focus) = model.focus.as_mut() {
        match key.code {
            KeyCode::Up => focus.scroll = focus.scroll.saturating_add(1),
            KeyCode::Down => focus.scroll = focus.scroll.saturating_sub(1),
            KeyCode::Esc => {
                // Background the window; a running deploy keeps running.
                if focus.done {
                    model.focus = None;
                } else {
                    model.status_line = "deploy keeps running — feed in LOG_STREAM".into();
                    model.focus = None;
                }
            }
            KeyCode::Enter if focus.done => model.focus = None,
            _ => {}
        }
        return;
    }
    if model.help_open {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('h') | KeyCode::Enter) {
            model.help_open = false;
        }
        return;
    }
    if model.wizard.is_some() {
        wizard_key(model, key);
        return;
    }
    if model.plan.is_some() {
        match key.code {
            KeyCode::Esc => model.plan = None,
            KeyCode::Enter => {
                // Execute the previewed deploy.
                if let Some(plan) = model.plan.take() {
                    model.focus = Some(Focus {
                        title: format!("DEPLOY {}", plan.stack),
                        feed: Vec::new(),
                        scroll: 0,
                        done: false,
                        ok: false,
                        result: String::new(),
                    });
                    model.outbox.push(Command::DeployStack(plan.spec));
                }
            }
            _ => {}
        }
        return;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Char('q'), _) => model.should_quit = true,
        // AZERTY: Ctrl+K for palette; also accept Ctrl+P.
        (KeyCode::Char('k'), true) | (KeyCode::Char('p'), true) => {
            model.palette_open = true;
            model.palette_input.clear();
            model.palette_sel = 0;
        }
        (KeyCode::F(2), _) => {
            model.fx = model.fx.cycle();
            model.status_line = format!("effects → {}", model.fx.label());
        }
        (KeyCode::Char('h'), false) => model.help_open = true,
        (KeyCode::Tab, _) => {
            let next = (model.tab.index() + 1) % Tab::ALL.len();
            model.switch_tab(Tab::ALL[next]);
        }
        (KeyCode::BackTab, _) => {
            let prev = (model.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len();
            model.switch_tab(Tab::ALL[prev]);
        }
        _ => {
            // AZERTY-friendly tab selection: digits 1-4 AND their unshifted
            // symbols on a Belgian/French AZERTY row (& é " ').
            if let KeyCode::Char(c) = key.code {
                if let Some(idx) = azerty_tab_index(c) {
                    if idx < Tab::ALL.len() {
                        model.switch_tab(Tab::ALL[idx]);
                        return;
                    }
                }
            }
            tab_key(model, key);
        }
    }
}

/// Map both the number row and the AZERTY unshifted symbols to a tab index.
pub fn azerty_tab_index(c: char) -> Option<usize> {
    match c {
        '1' | '&' => Some(0),
        '2' | 'é' => Some(1),
        '3' | '"' => Some(2),
        '4' | '\'' => Some(3),
        _ => None,
    }
}

fn tab_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    match model.tab {
        Tab::Dashboard | Tab::Stacks => match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                let n = model.stack_count();
                if n > 0 {
                    model.selected_stack = (model.selected_stack + 1) % n;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let n = model.stack_count();
                if n > 0 {
                    model.selected_stack = (model.selected_stack + n - 1) % n;
                }
            }
            KeyCode::Char('r') => model.outbox.push(Command::GetState),
            KeyCode::Char('D') => start_deploy(model),
            KeyCode::Char('p') => open_plan(model),
            KeyCode::Char('n') => {
                model.wizard = Some(Wizard {
                    step: WizStep::Preset,
                    preset_idx: 0,
                    name: String::new(),
                });
            }
            _ => {}
        },
        Tab::Logs => match key.code {
            KeyCode::Char(' ') => {
                model.log_follow = !model.log_follow;
                if model.log_follow {
                    model.log_scroll = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                model.log_scroll += 1;
                model.log_follow = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                model.log_scroll = model.log_scroll.saturating_sub(1);
                if model.log_scroll == 0 {
                    model.log_follow = true;
                }
            }
            KeyCode::Left => {
                let n = model.stack_count() + 1;
                model.log_source = (model.log_source + n - 1) % n;
            }
            KeyCode::Right => {
                let n = model.stack_count() + 1;
                model.log_source = (model.log_source + 1) % n;
            }
            KeyCode::Char('G') | KeyCode::End => {
                model.log_scroll = 0;
                model.log_follow = true;
            }
            _ => {}
        },
        Tab::Doctor => {
            if matches!(key.code, KeyCode::Char('r') | KeyCode::Enter) {
                model.outbox.push(Command::Doctor);
            }
        }
    }
}

pub struct PaletteAction {
    pub label: &'static str,
    pub id: &'static str,
}

pub const PALETTE: &[PaletteAction] = &[
    PaletteAction {
        label: "go: dashboard",
        id: "tab.dashboard",
    },
    PaletteAction {
        label: "go: stacks",
        id: "tab.stacks",
    },
    PaletteAction {
        label: "go: log stream",
        id: "tab.logs",
    },
    PaletteAction {
        label: "go: doctor",
        id: "tab.doctor",
    },
    PaletteAction {
        label: "refresh state",
        id: "refresh",
    },
    PaletteAction {
        label: "run doctor",
        id: "doctor",
    },
    PaletteAction {
        label: "cycle effects (F2)",
        id: "fx",
    },
    PaletteAction {
        label: "help",
        id: "help",
    },
    PaletteAction {
        label: "quit",
        id: "quit",
    },
];

pub fn palette_matches(input: &str) -> Vec<usize> {
    let q = input.to_lowercase();
    PALETTE
        .iter()
        .enumerate()
        .filter(|(_, a)| q.is_empty() || a.label.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

fn palette_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Esc => model.palette_open = false,
        KeyCode::Char(c) => {
            model.palette_input.push(c);
            model.palette_sel = 0;
        }
        KeyCode::Backspace => {
            model.palette_input.pop();
            model.palette_sel = 0;
        }
        KeyCode::Down => {
            let n = palette_matches(&model.palette_input).len();
            if n > 0 {
                model.palette_sel = (model.palette_sel + 1) % n;
            }
        }
        KeyCode::Up => {
            let n = palette_matches(&model.palette_input).len();
            if n > 0 {
                model.palette_sel = (model.palette_sel + n - 1) % n;
            }
        }
        KeyCode::Enter => {
            let matches = palette_matches(&model.palette_input);
            if let Some(&ai) = matches.get(model.palette_sel) {
                let id = PALETTE[ai].id;
                model.palette_open = false;
                run_action(model, id);
            }
        }
        _ => {}
    }
}

fn run_action(model: &mut Model, id: &str) {
    match id {
        "tab.dashboard" => model.switch_tab(Tab::Dashboard),
        "tab.stacks" => model.switch_tab(Tab::Stacks),
        "tab.logs" => model.switch_tab(Tab::Logs),
        "tab.doctor" => model.switch_tab(Tab::Doctor),
        "refresh" => model.outbox.push(Command::GetState),
        "doctor" => {
            model.switch_tab(Tab::Doctor);
            model.outbox.push(Command::Doctor);
        }
        "fx" => model.fx = model.fx.cycle(),
        "help" => model.help_open = true,
        "quit" => model.should_quit = true,
        _ => {}
    }
}

/// SHIFT+D: deploy the selected fleet stack from its local directory.
fn start_deploy(model: &mut Model) {
    let Some(fleet) = &model.fleet else {
        model.status_line = "no fleet state yet — press R first".into();
        return;
    };
    let Some(stack) = fleet.stacks.get(model.selected_stack) else {
        return;
    };
    let Some((_, dir)) = model.local_stacks.iter().find(|(n, _)| *n == stack.name) else {
        model.status_line = format!("no local stacks/{} directory to deploy from", stack.name);
        return;
    };
    match crate::spec::build_spec(dir) {
        Ok(spec) => {
            // D10: fail fast client-side.
            if let Err(e) = homelab_core::manifest::validate(&spec) {
                model.status_line = format!("validation failed: {}", e);
                return;
            }
            model.focus = Some(Focus {
                title: format!(
                    "DEPLOY {} :: vmid {}",
                    spec.manifest.stack_name, spec.manifest.vmid
                ),
                feed: Vec::new(),
                scroll: 0,
                done: false,
                ok: false,
                result: String::new(),
            });
            model.outbox.push(Command::DeployStack(Box::new(spec)));
        }
        Err(e) => model.status_line = e,
    }
}

/// P: build the D6 change-plan for the selected stack and show it. The plan is
/// computed locally from the spec vs the known runtime state.
fn open_plan(model: &mut Model) {
    let Some(fleet) = &model.fleet else {
        model.status_line = "no fleet state yet — press R first".into();
        return;
    };
    let Some(stack) = fleet.stacks.get(model.selected_stack) else {
        return;
    };
    let known = stack.online; // already provisioned in the fleet
    let Some((_, dir)) = model.local_stacks.iter().find(|(n, _)| *n == stack.name) else {
        model.status_line = format!("no local stacks/{} directory to plan from", stack.name);
        return;
    };
    let spec = match crate::spec::build_spec(dir) {
        Ok(s) => s,
        Err(e) => {
            model.status_line = e;
            return;
        }
    };
    if let Err(e) = homelab_core::manifest::validate(&spec) {
        model.status_line = format!("validation failed: {}", e);
        return;
    }
    let m = &spec.manifest;
    let mut lines: Vec<(char, String)> = Vec::new();
    lines.push((' ', "dry-run — nothing runs until you confirm".into()));
    lines.push((' ', String::new()));
    lines.push((' ', "plan:".into()));
    if known {
        lines.push((
            '~',
            format!("  UPDATE   {} (already provisioned)", m.hostname),
        ));
    } else {
        lines.push(('+', format!("  CREATE   {} (vmid {})", m.hostname, m.vmid)));
    }
    for app in &m.apps {
        lines.push(('~', format!("  SYNC     {}/{}", m.stack_name, app)));
    }
    lines.push((' ', String::new()));
    lines.push((
        ' ',
        format!(
            "payload: {} file(s), {} env(s)",
            spec.files.len(),
            spec.env.len()
        ),
    ));
    lines.push((' ', "safety:".into()));
    lines.push((' ', "  ✓ hostname guard verifies before any change".into()));
    lines.push((' ', "  ✓ fail-closed: errors disable the stack".into()));
    lines.push((' ', "  ✓ no-touch list protects 100-107,111,201-203".into()));
    model.plan = Some(Plan {
        stack: m.stack_name.clone(),
        lines,
        spec: Box::new(spec),
    });
}

/// Lowest free vmid in 108..=354 not used by a known stack.
pub fn next_free_vmid(model: &Model) -> u16 {
    let used: Vec<u16> = model
        .fleet
        .as_ref()
        .map(|f| f.stacks.iter().map(|s| s.vmid).collect())
        .unwrap_or_default();
    (108..=354).find(|v| !used.contains(v)).unwrap_or(354)
}

fn wizard_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    let vmid = next_free_vmid(model);
    let Some(w) = model.wizard.as_mut() else {
        return;
    };
    match w.step {
        WizStep::Preset => match key.code {
            KeyCode::Esc => model.wizard = None,
            KeyCode::Up => w.preset_idx = (w.preset_idx + PRESETS.len() - 1) % PRESETS.len(),
            KeyCode::Down => w.preset_idx = (w.preset_idx + 1) % PRESETS.len(),
            KeyCode::Enter => {
                if w.name.is_empty() {
                    w.name = PRESETS[w.preset_idx].name.to_string();
                }
                w.step = WizStep::Name;
            }
            _ => {}
        },
        WizStep::Name => match key.code {
            KeyCode::Esc => w.step = WizStep::Preset,
            KeyCode::Char(c) if c.is_ascii_alphanumeric() || c == '-' => {
                w.name.push(c.to_ascii_lowercase());
            }
            KeyCode::Backspace => {
                w.name.pop();
            }
            KeyCode::Enter if !w.name.is_empty() => w.step = WizStep::Review,
            _ => {}
        },
        WizStep::Review => match key.code {
            KeyCode::Esc => w.step = WizStep::Name,
            KeyCode::Enter => {
                let preset = &PRESETS[w.preset_idx];
                let name = w.name.clone();
                let ram = preset.ram;
                let app = preset.app;
                match crate::scaffold::scaffold_stack(
                    std::path::Path::new("stacks"),
                    &name,
                    vmid,
                    ram,
                    app,
                ) {
                    Ok(s) => {
                        model.status_line = format!(
                            "scaffolded stacks/{} ({} files) — press SHIFT+D to deploy",
                            name,
                            s.files.len()
                        );
                        // Make it immediately deployable and refresh.
                        model.local_stacks.push((name, s.dir));
                        model.local_stacks.sort();
                    }
                    Err(e) => model.status_line = format!("scaffold failed: {}", e),
                }
                model.wizard = None;
            }
            _ => {}
        },
    }
}
