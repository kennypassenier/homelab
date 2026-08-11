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
            ServerMsg::Log { level, source, msg } => model.push_log(level, source, msg),
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
    if model.help_open {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('h') | KeyCode::Enter) {
            model.help_open = false;
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
