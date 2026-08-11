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
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Dashboard,
        Tab::Stacks,
        Tab::Logs,
        Tab::Doctor,
        Tab::Settings,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "DASHBOARD",
            Tab::Stacks => "STACKS",
            Tab::Logs => "LOG_STREAM",
            Tab::Doctor => "DOCTOR",
            Tab::Settings => "SETTINGS",
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
    Resources,
    Review,
}

/// Which resources-form row is focused.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResField {
    Ram,
    Cores,
    Disk,
    Swap,
    Vmid,
}

pub struct Wizard {
    pub step: WizStep,
    pub preset_idx: usize,
    pub name: String,
    pub ram: u32,
    pub cores: u16,
    pub disk: u16,
    /// Auto-derived from RAM until the user touches it; 0 = no swap.
    pub swap: u32,
    pub swap_touched: bool,
    pub vmid: u16,
    pub res_field: ResField,
    /// True while typing a custom disk size digit-by-digit.
    pub disk_typing: bool,
}

/// RAM ladder: 256, 512, 1024, 2048, then +1024 up to 32768.
pub fn ram_step(current: u32, up: bool) -> u32 {
    const LOW: &[u32] = &[256, 512, 1024, 2048];
    if up {
        if current < 2048 {
            *LOW.iter().find(|&&v| v > current).unwrap_or(&2048)
        } else {
            (current + 1024).min(32768)
        }
    } else if current > 2048 {
        current - 1024
    } else {
        *LOW.iter().rev().find(|&&v| v < current).unwrap_or(&256)
    }
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

    /// G8 settings tab: last received host config, edit cursor, dirty flag,
    /// and the webhook text-edit buffer (None = not editing).
    pub settings: Option<homelab_proto::HostConfigView>,
    pub settings_row: usize,
    pub settings_dirty: bool,
    pub settings_editing_webhook: Option<String>,

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
            settings: None,
            settings_row: 0,
            settings_dirty: false,
            settings_editing_webhook: None,
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
            if tab == Tab::Settings && self.settings.is_none() {
                self.outbox.push(Command::GetConfig);
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
            ServerMsg::Config(view) => {
                model.settings = Some(*view);
                model.settings_dirty = false;
                model.settings_row = 0;
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

    // G8: webhook text-edit swallows every key (digits would switch tabs).
    if model.tab == Tab::Settings && model.settings_editing_webhook.is_some() {
        settings_webhook_edit_key(model, key);
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
        '5' | '(' => Some(4),
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
                let vmid = next_free_vmid(model);
                model.wizard = Some(Wizard {
                    step: WizStep::Preset,
                    preset_idx: 0,
                    name: String::new(),
                    ram: PRESETS[0].ram,
                    cores: 2,
                    disk: 8,
                    swap: crate::scaffold::StackDefaults::default().swap_for(PRESETS[0].ram),
                    swap_touched: false,
                    vmid,
                    res_field: ResField::Ram,
                    disk_typing: false,
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
        Tab::Settings => settings_key(model, key),
    }
}

// ── G8 settings tab logic ───────────────────────────────────────────────────

/// Row layout: 0 = backup hour; 1 + 2i / 2 + 2i = tier i every/span;
/// last = webhook. Total rows = 2 + tiers*2.
fn settings_rows(cfg: &homelab_proto::HostConfigView) -> usize {
    2 + cfg.retention.len() * 2
}

const EVERY_PRESETS: &[u32] = &[1, 2, 3, 7, 14, 21, 30, 45, 60, 90, 120, 180];
const SPAN_PRESETS: &[u32] = &[7, 14, 21, 30, 60, 90, 120, 180, 365, 730];

fn step_preset(presets: &[u32], current: u32, dir: i32) -> u32 {
    let pos = presets.iter().position(|p| *p >= current).unwrap_or(0);
    let next = (pos as i32 + dir).clamp(0, presets.len() as i32 - 1) as usize;
    presets[next]
}

fn settings_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    let Some(cfg) = model.settings.as_mut() else {
        if matches!(key.code, KeyCode::Char('r') | KeyCode::Enter) {
            model.outbox.push(Command::GetConfig);
        }
        return;
    };
    let rows = settings_rows(cfg);
    let webhook_row = rows - 1;
    let row = model.settings_row;
    match key.code {
        KeyCode::Up => model.settings_row = row.saturating_sub(1),
        KeyCode::Down => model.settings_row = (row + 1).min(rows - 1),
        KeyCode::Left | KeyCode::Right => {
            let dir: i32 = if key.code == KeyCode::Left { -1 } else { 1 };
            if row == 0 {
                // off, 0..23 cycle.
                cfg.backup_hour = match (cfg.backup_hour, dir) {
                    (None, 1) => Some(0),
                    (None, _) => Some(23),
                    (Some(0), -1) => None,
                    (Some(23), 1) => None,
                    (Some(h), 1) => Some(h + 1),
                    (Some(h), _) => Some(h - 1),
                };
            } else if row < webhook_row {
                let tier_idx = (row - 1) / 2;
                let is_every = (row - 1).is_multiple_of(2);
                if let Some(t) = cfg.retention.get_mut(tier_idx) {
                    if is_every {
                        t.every_days = step_preset(EVERY_PRESETS, t.every_days, dir);
                    } else {
                        // span cycles presets and 'forever' (None) at the top end.
                        t.span_days = match (t.span_days, dir) {
                            (None, -1) => Some(*SPAN_PRESETS.last().unwrap()),
                            (None, _) => None,
                            (Some(v), 1) if v >= *SPAN_PRESETS.last().unwrap() => None,
                            (Some(v), d) => Some(step_preset(SPAN_PRESETS, v, d)),
                        };
                    }
                }
            }
            model.settings_dirty = true;
        }
        KeyCode::Char('a') => {
            // Add a tier before any unbounded tail tier.
            let insert_at = cfg
                .retention
                .iter()
                .position(|t| t.span_days.is_none())
                .unwrap_or(cfg.retention.len());
            cfg.retention.insert(
                insert_at,
                homelab_proto::RetentionTier {
                    every_days: 30,
                    span_days: Some(90),
                },
            );
            model.settings_dirty = true;
        }
        KeyCode::Char('d') => {
            if row >= 1 && row < webhook_row && cfg.retention.len() > 1 {
                let tier_idx = (row - 1) / 2;
                cfg.retention.remove(tier_idx);
                model.settings_row = model.settings_row.min(settings_rows(cfg) - 1);
                model.settings_dirty = true;
            }
        }
        KeyCode::Enter if row == webhook_row => {
            model.settings_editing_webhook = Some(cfg.notify_webhook.clone().unwrap_or_default());
        }
        KeyCode::Char('S') => {
            model.outbox.push(Command::SetConfig(Box::new(cfg.clone())));
            model.settings_dirty = false;
            model.status_line = "settings sent to host".into();
        }
        KeyCode::Char('r') => model.outbox.push(Command::GetConfig),
        _ => {}
    }
}

fn settings_webhook_edit_key(model: &mut Model, key: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    let Some(buf) = model.settings_editing_webhook.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc => model.settings_editing_webhook = None,
        KeyCode::Enter => {
            let text = buf.trim().to_string();
            if let Some(cfg) = model.settings.as_mut() {
                cfg.notify_webhook = if text.is_empty() { None } else { Some(text) };
                model.settings_dirty = true;
            }
            model.settings_editing_webhook = None;
        }
        KeyCode::Backspace => {
            buf.pop();
        }
        KeyCode::Char(c) => buf.push(c),
        _ => {}
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

/// Resolve a deployable spec for the selected fleet stack: prefer a local
/// stacks/<name>/ directory; fall back to a synthetic spec derived from the
/// fleet state (used in the offline demo and for already-provisioned stacks
/// with no local dir). Returns (spec, is_synthetic).
fn resolve_spec(model: &Model) -> Result<(homelab_proto::DeploySpec, bool), String> {
    let fleet = model
        .fleet
        .as_ref()
        .ok_or("no fleet state yet — press R first")?;
    let stack = fleet
        .stacks
        .get(model.selected_stack)
        .ok_or("no stack selected")?;
    if let Some((_, dir)) = model.local_stacks.iter().find(|(n, _)| *n == stack.name) {
        let spec = crate::spec::build_spec(dir)?;
        homelab_core::manifest::validate(&spec).map_err(|e| format!("validation failed: {}", e))?;
        return Ok((spec, false));
    }
    // Synthetic spec from the fleet view — enough to preview/demo.
    let d = crate::scaffold::StackDefaults::default();
    let manifest = homelab_proto::StackManifest {
        stack_name: stack.name.clone(),
        vmid: stack.vmid,
        hostname: stack.hostname.clone(),
        network: homelab_proto::NetworkSpec {
            ip: format!(
                "{}{}/{}",
                d.ip_prefix,
                stack.vmid.saturating_sub(100),
                d.cidr
            ),
            gateway: d.gateway.clone(),
            bridge: d.bridge.clone(),
            vlan: Some(d.vlan),
        },
        resources: homelab_proto::ResourceSpec {
            cores: d.default_cores,
            memory_mb: 1024,
            swap_mb: d.swap_for(1024),
            disk_gb: d.default_disk_gb as u32,
            storage: d.storage.clone(),
        },
        lxc: homelab_proto::LxcSpec {
            template: d.template.clone(),
            unprivileged: d.unprivileged,
            features: d.features.clone(),
            protection: d.protection,
        },
        boot: homelab_proto::BootSpec {
            onboot: true,
            order: Some(d.boot_order),
        },
        storage: vec![],
        apps: stack.apps.iter().map(|a| a.name.clone()).collect(),
    };
    Ok((
        homelab_proto::DeploySpec {
            manifest,
            files: vec![],
            env: Default::default(),
            gateway_route: None,
        },
        true,
    ))
}

/// SHIFT+D: deploy the selected fleet stack.
fn start_deploy(model: &mut Model) {
    match resolve_spec(model) {
        Ok((spec, _synthetic)) => {
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

/// P: build the D6 change-plan for the selected stack and show it. Resolved
/// locally from the spec (real or synthetic) vs the known runtime state.
fn open_plan(model: &mut Model) {
    let known = model
        .fleet
        .as_ref()
        .and_then(|f| f.stacks.get(model.selected_stack))
        .map(|s| s.online)
        .unwrap_or(false);
    let (spec, synthetic) = match resolve_spec(model) {
        Ok(v) => v,
        Err(e) => {
            model.status_line = e;
            return;
        }
    };
    let m = &spec.manifest;
    let mut lines: Vec<(char, String)> = Vec::new();
    lines.push((' ', "dry-run — nothing runs until you confirm".into()));
    if synthetic {
        lines.push((
            ' ',
            "(no local stacks/ dir — plan derived from live state)".into(),
        ));
    }
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
            KeyCode::Enter if !w.name.is_empty() => {
                w.ram = PRESETS[w.preset_idx].ram;
                w.step = WizStep::Resources;
            }
            _ => {}
        },
        WizStep::Resources => match key.code {
            KeyCode::Esc => w.step = WizStep::Name,
            KeyCode::Up => {
                w.res_field = match w.res_field {
                    ResField::Ram => ResField::Vmid,
                    ResField::Cores => ResField::Ram,
                    ResField::Disk => ResField::Cores,
                    ResField::Swap => ResField::Disk,
                    ResField::Vmid => ResField::Swap,
                };
                w.disk_typing = false;
            }
            KeyCode::Down => {
                w.res_field = match w.res_field {
                    ResField::Ram => ResField::Cores,
                    ResField::Cores => ResField::Disk,
                    ResField::Disk => ResField::Swap,
                    ResField::Swap => ResField::Vmid,
                    ResField::Vmid => ResField::Ram,
                };
                w.disk_typing = false;
            }
            KeyCode::Right => {
                w.disk_typing = false;
                match w.res_field {
                    ResField::Ram => {
                        w.ram = ram_step(w.ram, true);
                        if !w.swap_touched {
                            w.swap = crate::scaffold::StackDefaults::default().swap_for(w.ram);
                        }
                    }
                    ResField::Cores => w.cores = (w.cores + 1).min(16),
                    ResField::Disk => w.disk = (w.disk + 2).min(999),
                    ResField::Swap => {
                        w.swap = (w.swap + 256).min(4096);
                        w.swap_touched = true;
                    }
                    ResField::Vmid => w.vmid = (w.vmid + 1).min(354),
                }
            }
            KeyCode::Left => {
                w.disk_typing = false;
                match w.res_field {
                    ResField::Ram => {
                        w.ram = ram_step(w.ram, false);
                        if !w.swap_touched {
                            w.swap = crate::scaffold::StackDefaults::default().swap_for(w.ram);
                        }
                    }
                    ResField::Cores => w.cores = w.cores.saturating_sub(1).max(1),
                    ResField::Disk => w.disk = w.disk.saturating_sub(2).max(2),
                    ResField::Swap => {
                        w.swap = w.swap.saturating_sub(256);
                        w.swap_touched = true;
                    }
                    ResField::Vmid => w.vmid = w.vmid.saturating_sub(1).max(108),
                }
            }
            // Typed custom disk size.
            KeyCode::Char(c) if w.res_field == ResField::Disk && c.is_ascii_digit() => {
                if !w.disk_typing {
                    w.disk = 0;
                    w.disk_typing = true;
                }
                w.disk = (w.disk * 10 + c.to_digit(10).unwrap() as u16).min(999);
            }
            KeyCode::Backspace if w.res_field == ResField::Disk => {
                w.disk_typing = true;
                w.disk /= 10;
            }
            KeyCode::Enter => {
                if w.disk < 2 {
                    w.disk = 2;
                }
                w.disk_typing = false;
                w.step = WizStep::Review;
            }
            _ => {}
        },
        WizStep::Review => match key.code {
            KeyCode::Esc => w.step = WizStep::Resources,
            KeyCode::Enter => {
                let preset = &PRESETS[w.preset_idx];
                let name = w.name.clone();
                let (ram, cores, disk, swap, vmid) = (w.ram, w.cores, w.disk, w.swap, w.vmid);
                let app = preset.app;
                match crate::scaffold::scaffold_stack(
                    std::path::Path::new("stacks"),
                    &crate::scaffold::StackParams {
                        name: &name,
                        vmid,
                        ram_mb: ram,
                        cores,
                        disk_gb: disk,
                        swap_mb: Some(swap),
                        app,
                    },
                ) {
                    Ok(s) => {
                        model.status_line = format!(
                            "scaffolded stacks/{} ({} files) — press SHIFT+D to deploy",
                            name,
                            s.files.len()
                        );
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
