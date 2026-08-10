//! Application state machine: screens, tabs, modals, wizard, command palette.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::fx::FxLevel;
use crate::sim::{Level, World};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Splash,
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Stacks,
    Backups,
    Logs,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Dashboard, Tab::Stacks, Tab::Backups, Tab::Logs];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "DASHBOARD",
            Tab::Stacks => "STACKS",
            Tab::Backups => "BACKUPS",
            Tab::Logs => "LOGS",
        }
    }
    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

// ── Wizard ───────────────────────────────────────────────────────────────────

pub struct Preset {
    pub name: &'static str,
    pub desc: &'static str,
    pub apps: &'static [(&'static str, &'static str)],
    pub ram: u32,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Syncthing",
        desc: "Obsidian vault always-on peer",
        apps: &[("syncthing", "syncthing/syncthing:latest")],
        ram: 512,
    },
    Preset {
        name: "Jellyfin",
        desc: "Media server (VAAPI transcode)",
        apps: &[("jellyfin", "jellyfin/jellyfin:latest")],
        ram: 4096,
    },
    Preset {
        name: "*arr pack",
        desc: "Sonarr + Radarr + Prowlarr + Bazarr",
        apps: &[
            ("sonarr", "linuxserver/sonarr:latest"),
            ("radarr", "linuxserver/radarr:latest"),
            ("prowlarr", "linuxserver/prowlarr:latest"),
            ("bazarr", "linuxserver/bazarr:latest"),
        ],
        ram: 2048,
    },
    Preset {
        name: "VPN downloads",
        desc: "Gluetun + qBittorrent",
        apps: &[
            ("gluetun", "qmcgaw/gluetun:latest"),
            ("qbittorrent", "linuxserver/qbittorrent:latest"),
        ],
        ram: 2048,
    },
    Preset {
        name: "Mealie",
        desc: "Recipes + meal planning (HA integration)",
        apps: &[("mealie", "ghcr.io/mealie-recipes/mealie:latest")],
        ram: 512,
    },
    Preset {
        name: "Actual Budget",
        desc: "Envelope budgeting, local-first",
        apps: &[("actual", "actualbudget/actual-server:latest")],
        ram: 512,
    },
    Preset {
        name: "Stirling PDF",
        desc: "Merge/split/OCR/sign PDFs, local",
        apps: &[("stirling-pdf", "frooodle/s-pdf:latest")],
        ram: 1024,
    },
    Preset {
        name: "Uptime Kuma",
        desc: "Uptime monitoring",
        apps: &[("uptime-kuma", "louislam/uptime-kuma:1")],
        ram: 512,
    },
    Preset {
        name: "Custom",
        desc: "Empty stack — add apps later",
        apps: &[],
        ram: 1024,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    Preset,
    Name,
    Resources,
    Review,
}

pub struct WizardState {
    pub step: WizardStep,
    pub preset_idx: usize,
    pub name: String,
    pub ram: u32,
    pub cores: u16,
    pub disk: u16,
}

impl WizardState {
    pub fn new() -> Self {
        Self {
            step: WizardStep::Preset,
            preset_idx: 0,
            name: String::new(),
            ram: PRESETS[0].ram,
            cores: 2,
            disk: 8,
        }
    }
}

// ── Modals ───────────────────────────────────────────────────────────────────

pub enum Modal {
    None,
    Help,
    Wizard(WizardState),
    Diff { stack_idx: usize, scroll: u16 },
    Deploy,
    ConfirmDelete { stack_idx: usize, input: String },
}

// ── Command palette ─────────────────────────────────────────────────────────

pub struct PaletteAction {
    pub label: &'static str,
    pub id: &'static str,
}

pub const PALETTE_ACTIONS: &[PaletteAction] = &[
    PaletteAction { label: "go: dashboard", id: "tab.dashboard" },
    PaletteAction { label: "go: stacks", id: "tab.stacks" },
    PaletteAction { label: "go: backups", id: "tab.backups" },
    PaletteAction { label: "go: logs", id: "tab.logs" },
    PaletteAction { label: "stack: new (wizard)", id: "stack.new" },
    PaletteAction { label: "stack: deploy selected", id: "stack.deploy" },
    PaletteAction { label: "backup: run for selected stack", id: "backup.run" },
    PaletteAction { label: "fx: cycle effect intensity", id: "fx.cycle" },
    PaletteAction { label: "help: show keymap", id: "help" },
    PaletteAction { label: "quit", id: "quit" },
];

pub struct PaletteState {
    pub open: bool,
    pub input: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn matches(&self) -> Vec<usize> {
        let q = self.input.to_lowercase();
        PALETTE_ACTIONS
            .iter()
            .enumerate()
            .filter(|(_, a)| q.is_empty() || a.label.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }
}

// ── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    pub world: World,
    pub screen: Screen,
    pub tab: Tab,
    pub fx: FxLevel,
    pub tick: u64,        // anim ticks (~30/s)
    pub reveal_start: u64, // decrypt-reveal anchor for current screen/tab
    pub flicker: u8,      // power-cycle countdown on tab switch
    pub boot_skipped: bool,

    pub stack_table: TableState,
    pub app_table: TableState,
    pub snap_table: TableState,
    pub palette: PaletteState,
    pub modal: Modal,

    pub logs_follow: bool,
    pub log_filter: Option<Level>,
    /// 0 = ALL, 1 = HOST, 2 = CLIENT, 3.. = stack index + 3.
    pub log_source: usize,
    /// Offset from the tail; 0 means following the live end.
    pub log_scroll: usize,
    /// Scroll offset within the deploy focus window (0 = live tail).
    pub deploy_scroll: usize,
    /// Last observed deploy-log length, used to anchor a scrolled-back view.
    pub deploy_log_seen: usize,
    pub should_quit: bool,
    pub status_line: String,
}

impl App {
    pub fn new() -> Self {
        let mut stack_table = TableState::default();
        stack_table.select(Some(0));
        Self {
            world: World::new(),
            screen: Screen::Splash,
            tab: Tab::Dashboard,
            fx: FxLevel::Full,
            tick: 0,
            reveal_start: 0,
            flicker: 0,
            boot_skipped: false,
            stack_table,
            app_table: TableState::default(),
            snap_table: TableState::default(),
            palette: PaletteState { open: false, input: String::new(), selected: 0 },
            modal: Modal::None,
            logs_follow: true,
            log_filter: None,
            log_source: 0,
            log_scroll: 0,
            deploy_scroll: 0,
            deploy_log_seen: 0,
            should_quit: false,
            status_line: "boot sequence complete — all systems nominal".into(),
        }
    }

    pub fn selected_stack(&self) -> usize {
        self.stack_table.selected().unwrap_or(0).min(self.world.stacks.len().saturating_sub(1))
    }

    /// Number of log-source slots: ALL + HOST + CLIENT + one per stack.
    pub fn log_source_count(&self) -> usize {
        3 + self.world.stacks.len()
    }

    pub fn log_source_name(&self, idx: usize) -> String {
        match idx {
            0 => "ALL".into(),
            1 => "HOST".into(),
            2 => "CLIENT".into(),
            i => self
                .world
                .stacks
                .get(i - 3)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "?".into()),
        }
    }

    /// Does a log line pass the current source selection?
    pub fn log_source_matches(&self, source: &str) -> bool {
        match self.log_source {
            0 => true,
            1 => source == "HOST",
            2 => source == "CLIENT",
            i => self
                .world
                .stacks
                .get(i - 3)
                .map(|s| s.name == source)
                .unwrap_or(false),
        }
    }

    pub fn tick_anim(&mut self) {
        self.tick += 1;
        if self.flicker > 0 {
            self.flicker -= 1;
        }
        // Anchor the deploy focus window while scrolled back: new transcript
        // lines must not shift what the user is reading.
        if let Some(d) = &self.world.deploy {
            let len = d.log.len();
            if self.deploy_scroll > 0 {
                self.deploy_scroll += len.saturating_sub(self.deploy_log_seen);
            }
            self.deploy_log_seen = len;
        } else {
            self.deploy_log_seen = 0;
        }
        // Auto-leave splash after the boot sequence has fully played (~4.5s).
        if self.screen == Screen::Splash && self.tick > 135 {
            self.enter_main();
        }
    }

    fn enter_main(&mut self) {
        self.screen = Screen::Main;
        self.reveal_start = self.tick;
        self.flicker = 4;
    }

    fn switch_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.reveal_start = self.tick;
            self.flicker = 4;
        }
    }

    pub fn reveal_progress(&self) -> f32 {
        ((self.tick.saturating_sub(self.reveal_start)) as f32 / 9.0).min(1.0)
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.screen == Screen::Splash {
            self.boot_skipped = true;
            self.enter_main();
            return;
        }

        // Command palette swallows keys while open.
        if self.palette.open {
            self.palette_key(key);
            return;
        }
        // Modals swallow keys while open.
        if !matches!(self.modal, Modal::None) {
            self.modal_key(key);
            return;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => self.should_quit = true,
            (KeyCode::Char('k'), KeyModifiers::CONTROL) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.palette = PaletteState { open: true, input: String::new(), selected: 0 };
            }
            (KeyCode::F(2), _) => {
                self.fx = self.fx.cycle();
                self.status_line = format!("effects → {}", self.fx.label());
            }
            (KeyCode::Char('?'), _) => self.modal = Modal::Help,
            (KeyCode::Char('1'), _) => self.switch_tab(Tab::Dashboard),
            (KeyCode::Char('2'), _) => self.switch_tab(Tab::Stacks),
            (KeyCode::Char('3'), _) => self.switch_tab(Tab::Backups),
            (KeyCode::Char('4'), _) => self.switch_tab(Tab::Logs),
            (KeyCode::Tab, _) => {
                let next = (self.tab.index() + 1) % Tab::ALL.len();
                self.switch_tab(Tab::ALL[next]);
            }
            (KeyCode::BackTab, _) => {
                let prev = (self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len();
                self.switch_tab(Tab::ALL[prev]);
            }
            _ => self.tab_key(key),
        }
    }

    fn move_sel(state: &mut TableState, len: usize, delta: i64) {
        if len == 0 {
            return;
        }
        let cur = state.selected().unwrap_or(0) as i64;
        let next = (cur + delta).rem_euclid(len as i64) as usize;
        state.select(Some(next));
    }

    fn tab_key(&mut self, key: KeyEvent) {
        let nstacks = self.world.stacks.len();
        match self.tab {
            Tab::Dashboard | Tab::Stacks => match key.code {
                KeyCode::Char('j') | KeyCode::Down => Self::move_sel(&mut self.stack_table, nstacks, 1),
                KeyCode::Char('k') | KeyCode::Up => Self::move_sel(&mut self.stack_table, nstacks, -1),
                KeyCode::Char('n') => self.modal = Modal::Wizard(WizardState::new()),
                KeyCode::Char('D') => {
                    // A live deploy reopens its focus window; otherwise start
                    // with the change-plan preview.
                    if self.world.deploy.as_ref().map(|d| !d.finished).unwrap_or(false) {
                        self.modal = Modal::Deploy;
                    } else {
                        let idx = self.selected_stack();
                        self.modal = Modal::Diff { stack_idx: idx, scroll: 0 };
                    }
                }
                KeyCode::Char('b') => {
                    let idx = self.selected_stack();
                    self.world.start_backup(idx);
                    self.switch_tab(Tab::Backups);
                }
                KeyCode::Char('a') => {
                    let idx = self.selected_stack();
                    if let Some(s) = self.world.stacks.get_mut(idx) {
                        s.enabled = true;
                        self.status_line = format!("stack {} :: deploy.enabled = true", s.name);
                    }
                }
                KeyCode::Char('x') => {
                    let idx = self.selected_stack();
                    if let Some(s) = self.world.stacks.get_mut(idx) {
                        s.enabled = false;
                        self.status_line = format!("stack {} :: deploy.enabled = false", s.name);
                    }
                }
                KeyCode::Char('d') | KeyCode::Delete => {
                    let idx = self.selected_stack();
                    self.modal = Modal::ConfirmDelete { stack_idx: idx, input: String::new() };
                }
                KeyCode::Enter => {
                    if self.tab == Tab::Dashboard {
                        self.switch_tab(Tab::Stacks);
                    }
                }
                _ => {}
            },
            Tab::Backups => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    Self::move_sel(&mut self.snap_table, self.world.snapshots.len(), 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    Self::move_sel(&mut self.snap_table, self.world.snapshots.len(), -1)
                }
                KeyCode::Char('b') => {
                    let idx = self.selected_stack();
                    self.world.start_backup(idx);
                }
                _ => {}
            },
            Tab::Logs => match key.code {
                KeyCode::Char(' ') => {
                    self.logs_follow = !self.logs_follow;
                    if self.logs_follow {
                        self.log_scroll = 0;
                    }
                    self.status_line = if self.logs_follow {
                        "logs :: follow ON".into()
                    } else {
                        "logs :: follow PAUSED".into()
                    };
                }
                KeyCode::Char('f') => {
                    self.log_filter = match self.log_filter {
                        None => Some(Level::Info),
                        Some(Level::Info) => Some(Level::Warn),
                        Some(Level::Warn) => Some(Level::Error),
                        Some(Level::Error) => Some(Level::Debug),
                        Some(Level::Debug) => None,
                    };
                    self.status_line = format!(
                        "logs :: filter {}",
                        self.log_filter.map(|l| l.label()).unwrap_or("OFF")
                    );
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let n = self.log_source_count();
                    self.log_source = (self.log_source + 1) % n;
                    self.log_scroll = 0;
                    self.status_line = format!("logs :: source {}", self.log_source_name(self.log_source));
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    let n = self.log_source_count();
                    self.log_source = (self.log_source + n - 1) % n;
                    self.log_scroll = 0;
                    self.status_line = format!("logs :: source {}", self.log_source_name(self.log_source));
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.log_scroll = self.log_scroll.saturating_add(1);
                    self.logs_follow = false;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                    if self.log_scroll == 0 {
                        self.logs_follow = true;
                    }
                }
                KeyCode::PageUp => {
                    self.log_scroll = self.log_scroll.saturating_add(15);
                    self.logs_follow = false;
                }
                KeyCode::PageDown => {
                    self.log_scroll = self.log_scroll.saturating_sub(15);
                    if self.log_scroll == 0 {
                        self.logs_follow = true;
                    }
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.log_scroll = 0;
                    self.logs_follow = true;
                    self.status_line = "logs :: jumped to tail, follow ON".into();
                }
                _ => {}
            },
        }
    }

    fn modal_key(&mut self, key: KeyEvent) {
        match &mut self.modal {
            Modal::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Enter) {
                    self.modal = Modal::None;
                }
            }
            Modal::Diff { stack_idx, scroll } => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Char('j') | KeyCode::Down => *scroll += 1,
                KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Enter => {
                    let idx = *stack_idx;
                    self.world.start_deploy(idx);
                    self.modal = Modal::Deploy;
                }
                _ => {}
            },
            Modal::Deploy => {
                let finished = self.world.deploy.as_ref().map(|d| d.finished).unwrap_or(true);
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.deploy_scroll = self.deploy_scroll.saturating_add(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.deploy_scroll = self.deploy_scroll.saturating_sub(1);
                    }
                    KeyCode::PageUp => {
                        self.deploy_scroll = self.deploy_scroll.saturating_add(10);
                    }
                    KeyCode::PageDown => {
                        self.deploy_scroll = self.deploy_scroll.saturating_sub(10);
                    }
                    KeyCode::Char('G') | KeyCode::End => self.deploy_scroll = 0,
                    KeyCode::Esc => {
                        self.modal = Modal::None;
                        self.deploy_scroll = 0;
                        if finished {
                            self.world.deploy = None;
                        }
                    }
                    KeyCode::Enter if finished => {
                        self.modal = Modal::None;
                        self.deploy_scroll = 0;
                        self.world.deploy = None;
                    }
                    _ => {}
                }
            }
            Modal::ConfirmDelete { stack_idx, input } => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Char(c) => input.push(c),
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    let idx = *stack_idx;
                    let expected = self.world.stacks.get(idx).map(|s| s.name.clone()).unwrap_or_default();
                    if *input == expected {
                        self.world.remove_stack(idx);
                        let len = self.world.stacks.len();
                        if len > 0 {
                            self.stack_table.select(Some(idx.min(len - 1)));
                        }
                        self.modal = Modal::None;
                        self.status_line = format!("stack {} removed from repo", expected);
                    }
                }
                _ => {}
            },
            Modal::Wizard(w) => match key.code {
                KeyCode::Esc => match w.step {
                    WizardStep::Preset => self.modal = Modal::None,
                    WizardStep::Name => w.step = WizardStep::Preset,
                    WizardStep::Resources => w.step = WizardStep::Name,
                    WizardStep::Review => w.step = WizardStep::Resources,
                },
                KeyCode::Char('j') | KeyCode::Down if w.step == WizardStep::Preset => {
                    w.preset_idx = (w.preset_idx + 1) % PRESETS.len();
                    w.ram = PRESETS[w.preset_idx].ram;
                }
                KeyCode::Char('k') | KeyCode::Up if w.step == WizardStep::Preset => {
                    w.preset_idx = (w.preset_idx + PRESETS.len() - 1) % PRESETS.len();
                    w.ram = PRESETS[w.preset_idx].ram;
                }
                KeyCode::Char(c) if w.step == WizardStep::Name => {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        w.name.push(c.to_ascii_lowercase());
                    }
                }
                KeyCode::Backspace if w.step == WizardStep::Name => {
                    w.name.pop();
                }
                KeyCode::Char('+') | KeyCode::Char('=') if w.step == WizardStep::Resources => {
                    w.ram = (w.ram + 512).min(16384);
                }
                KeyCode::Char('-') if w.step == WizardStep::Resources => {
                    w.ram = w.ram.saturating_sub(512).max(256);
                }
                KeyCode::Char('c') if w.step == WizardStep::Resources => {
                    w.cores = if w.cores >= 8 { 1 } else { w.cores + 1 };
                }
                KeyCode::Char('s') if w.step == WizardStep::Resources => {
                    w.disk = if w.disk >= 64 { 4 } else { w.disk * 2 };
                }
                KeyCode::Enter => match w.step {
                    WizardStep::Preset => {
                        if w.name.is_empty() {
                            w.name = PRESETS[w.preset_idx]
                                .name
                                .to_lowercase()
                                .replace([' ', '*'], "");
                        }
                        w.step = WizardStep::Name;
                    }
                    WizardStep::Name => {
                        if !w.name.is_empty() {
                            w.step = WizardStep::Resources;
                        }
                    }
                    WizardStep::Resources => w.step = WizardStep::Review,
                    WizardStep::Review => {
                        let preset = &PRESETS[w.preset_idx];
                        let name = w.name.clone();
                        let ram = w.ram;
                        self.world.add_stack(&name, preset.apps, ram);
                        self.modal = Modal::None;
                        self.switch_tab(Tab::Stacks);
                        self.stack_table.select(Some(self.world.stacks.len() - 1));
                        self.status_line =
                            format!("stack {} scaffolded — activate with [a], deploy with [D]", name);
                    }
                },
                _ => {}
            },
            Modal::None => {}
        }
    }

    fn palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.palette.open = false,
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.palette.input.push(c);
                self.palette.selected = 0;
            }
            KeyCode::Backspace => {
                self.palette.input.pop();
                self.palette.selected = 0;
            }
            KeyCode::Down => {
                let n = self.palette.matches().len();
                if n > 0 {
                    self.palette.selected = (self.palette.selected + 1) % n;
                }
            }
            KeyCode::Up => {
                let n = self.palette.matches().len();
                if n > 0 {
                    self.palette.selected = (self.palette.selected + n - 1) % n;
                }
            }
            KeyCode::Enter => {
                let matches = self.palette.matches();
                if let Some(&action_idx) = matches.get(self.palette.selected) {
                    let id = PALETTE_ACTIONS[action_idx].id;
                    self.palette.open = false;
                    self.run_action(id);
                }
            }
            _ => {}
        }
    }

    fn run_action(&mut self, id: &str) {
        match id {
            "tab.dashboard" => self.switch_tab(Tab::Dashboard),
            "tab.stacks" => self.switch_tab(Tab::Stacks),
            "tab.backups" => self.switch_tab(Tab::Backups),
            "tab.logs" => self.switch_tab(Tab::Logs),
            "stack.new" => self.modal = Modal::Wizard(WizardState::new()),
            "stack.deploy" => {
                let idx = self.selected_stack();
                self.modal = Modal::Diff { stack_idx: idx, scroll: 0 };
            }
            "backup.run" => {
                let idx = self.selected_stack();
                self.world.start_backup(idx);
                self.switch_tab(Tab::Backups);
            }
            "fx.cycle" => {
                self.fx = self.fx.cycle();
                self.status_line = format!("effects → {}", self.fx.label());
            }
            "help" => self.modal = Modal::Help,
            "quit" => self.should_quit = true,
            _ => {}
        }
    }
}
