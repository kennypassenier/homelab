//! Application state — Tab enum, App struct, and all state-management helpers.

use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::blast_radius::ActiveModal;
use crate::theme::Theme;

/// Represents the four main navigation tabs.
#[derive(Copy, Clone, Debug)]
pub enum Tab {
    Dashboard,
    Scaffolding,
    HostManagement,
    Logs,
}

impl Tab {
    /// Returns every tab in display order.
    pub fn all() -> &'static [Tab] {
        &[Tab::Dashboard, Tab::Scaffolding, Tab::HostManagement, Tab::Logs]
    }

    /// Human-readable tab label used in the tab bar.
    pub fn title(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Scaffolding => "Scaffolding",
            Tab::HostManagement => "Host Management",
            Tab::Logs => "Logs",
        }
    }
}

/// A single telemetry line stored in the Logs tab ring buffer.
pub struct LogLine {
    pub time: String,
    pub source: String,
    pub level: String,
    pub message: String,
}

/// Cyclic mock telemetry entries used until a live SSE stream is connected.
/// Each tuple is (source, level, message).
const MOCK_SEQUENCE: &[(&str, &str, &str)] = &[
    ("lxc-cloudflared", "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-cloudflared", "INFO",  "[git] Already up to date"),
    ("lxc-cloudflared", "INFO",  "[docker] cloudflared: Image up to date"),
    ("lxc-downloader",  "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-downloader",  "INFO",  "[git] Already up to date"),
    ("lxc-downloader",  "INFO",  "[docker] qbittorrent: Image up to date"),
    ("lxc-downloader",  "INFO",  "[promtail] Shipping 17 log lines to Loki"),
    ("lxc-gateway",     "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-gateway",     "INFO",  "[git] Already up to date"),
    ("lxc-gateway",     "INFO",  "[docker] crowdsec: Image up to date"),
    ("lxc-gateway",     "WARN",  "[crowdsec] Decision added: ban 203.0.113.42 (SSH bruteforce, 48 attempts)"),
    ("lxc-gateway",     "INFO",  "[docker] nginx-proxy-manager: Image up to date"),
    ("lxc-gateway",     "INFO",  "[promtail] Shipping 31 log lines to Loki"),
    ("lxc-media",       "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-media",       "INFO",  "[git] Fast-forward: 1 new commit"),
    ("lxc-media",       "INFO",  "[pre-sync] Running pre-sync.sh..."),
    ("lxc-media",       "INFO",  "[docker] bazarr: Pulling newer image"),
    ("lxc-media",       "INFO",  "[docker] bazarr: Container recreated"),
    ("lxc-media",       "INFO",  "[docker] bazarr: Started"),
    ("lxc-media",       "INFO",  "[docker] jellyfin: Image up to date"),
    ("lxc-media",       "INFO",  "[promtail] Shipping 52 log lines to Loki"),
    ("lxc-monitoring",  "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-monitoring",  "INFO",  "[git] Already up to date"),
    ("lxc-monitoring",  "INFO",  "[loki] Received 100 lines from 4 sources"),
    ("lxc-monitoring",  "INFO",  "[grafana] Dashboard scraped: uptime-kuma"),
    ("lxc-monitoring",  "INFO",  "[promtail] Shipping 8 log lines to Loki"),
    ("lxc-paperless",   "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-paperless",   "INFO",  "[git] Already up to date"),
    ("lxc-vikunja",     "INFO",  "[node-sync] Checking for upstream changes..."),
    ("lxc-vikunja",     "INFO",  "[git] Already up to date"),
    ("HOST",            "INFO",  "sync-host.sh: Syncing bind-mounts to lxc-media"),
    ("HOST",            "INFO",  "sync-host.sh: Sync complete"),
    ("lxc-media",       "INFO",  "[watchtower] Checking all images for updates..."),
    ("lxc-media",       "INFO",  "[watchtower] All containers up to date"),
    ("lxc-downloader",  "INFO",  "[qbittorrent] Torrent completed: ubuntu-24.04.iso (4.6 GB)"),
    ("lxc-gateway",     "ERROR", "[nginx-proxy-manager] Upstream unreachable: lxc-vikunja:3456 (timeout)"),
    ("lxc-gateway",     "WARN",  "[crowdsec] 5 new IPs banned this cycle"),
    ("CLIENT",          "INFO",  "Git push triggered: feat(media): add bazarr"),
    ("lxc-media",       "INFO",  "[node-sync] Fast-forward: 1 new commit"),
    ("lxc-media",       "INFO",  "[pre-sync] Running pre-sync.sh..."),
    ("lxc-media",       "INFO",  "[docker] bazarr: Pulling image"),
    ("lxc-media",       "INFO",  "[docker] bazarr: Container created and started"),
];

/// Returns the current wall-clock time as HH:MM:SS.
fn current_time_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// Tracks the collapsed/expanded state of an individual application row.
#[allow(dead_code)]
pub struct AppDropdown {
    pub expanded: bool,
    pub selected_option: usize,
}

/// Tracks the full dropdown state for one stack, including its apps.
pub struct StackDropdown {
    pub expanded: bool,
    pub selected_option: usize,
    pub apps: Vec<String>,
    pub app_dropdowns: Vec<AppDropdown>,
}

/// All runtime state for the TUI application.
pub struct App {
    pub active_tab: usize,
    pub theme: Theme,
    pub modal: ActiveModal,
    pub stacks: Vec<String>,
    pub selected_stack: usize,
    pub stack_dropdowns: Vec<StackDropdown>,
    /// 0 = stacks column, 1 = actions column, 2 = apps column
    pub column_focus: usize,
    pub stack_scroll: usize,
    /// Ring buffer of telemetry lines shown in the Logs tab.
    pub logs: Vec<LogLine>,
    /// Lines from the bottom to scroll back by (0 = live / pinned to newest).
    pub log_scroll: usize,
    /// Index into MOCK_SEQUENCE, advances on every tick.
    log_tick: usize,
}

impl App {
    /// Creates a freshly initialised application, loading stacks from disk.
    pub fn new() -> Self {
        let stacks = App::load_stacks();
        let stack_dropdowns = Self::build_dropdowns(&stacks);
        let mut app = Self {
            active_tab: 0,
            theme: Theme::cyberpunk(),
            modal: ActiveModal::None,
            stacks,
            selected_stack: 0,
            stack_dropdowns,
            column_focus: 0,
            stack_scroll: 0,
            logs: Vec::new(),
            log_scroll: 0,
            log_tick: 0,
        };
        // Pre-fill with a page of mock entries so the Logs tab is not blank on startup.
        for _ in 0..20 {
            app.tick_logs();
        }
        app
    }

    /// Scans `stacks/` (relative to the binary's CWD) for infrastructure stacks.
    pub fn load_stacks() -> Vec<String> {
        let mut result = Vec::new();
        if let Ok(entries) = fs::read_dir("stacks") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        result.push(name.to_string());
                    }
                }
            }
        }
        result.sort();
        result
    }

    /// Rebuilds stacks and dropdown state from disk (called after create/delete).
    pub fn reload_stacks_and_dropdowns(&mut self) {
        self.stacks = App::load_stacks();
        self.stack_dropdowns = Self::build_dropdowns(&self.stacks);
        if self.selected_stack >= self.stacks.len() && !self.stacks.is_empty() {
            self.selected_stack = self.stacks.len() - 1;
        }
    }

    /// Advances to the next tab (wraps around).
    pub fn tab_right(&mut self) {
        self.active_tab = (self.active_tab + 1) % Tab::all().len();
    }

    /// Retreats to the previous tab (wraps around).
    pub fn tab_left(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = Tab::all().len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }

    /// Returns the currently active `Tab` variant.
    pub fn active_tab(&self) -> Tab {
        Tab::all()[self.active_tab]
    }

    /// Adds the next mock log entry to the ring buffer.
    ///
    /// Once the buffer exceeds 500 lines the oldest entry is dropped.
    /// Auto-scrolling is only applied when the user has not manually scrolled up
    /// (i.e. `log_scroll == 0`).
    pub fn tick_logs(&mut self) {
        let (source, level, message) = MOCK_SEQUENCE[self.log_tick % MOCK_SEQUENCE.len()];
        self.logs.push(LogLine {
            time: current_time_str(),
            source: source.to_string(),
            level: level.to_string(),
            message: message.to_string(),
        });
        if self.logs.len() > 500 {
            self.logs.remove(0);
        }
        self.log_tick += 1;
    }

    // ── private helpers ─────────────────────────────────────────────────────

    fn build_dropdowns(stacks: &[String]) -> Vec<StackDropdown> {
        stacks
            .iter()
            .map(|name| {
                let apps = crate::app_list::list_apps_for_stack(name);
                let app_dropdowns = apps
                    .iter()
                    .map(|_| AppDropdown {
                        expanded: false,
                        selected_option: 0,
                    })
                    .collect();
                StackDropdown {
                    expanded: false,
                    selected_option: 0,
                    apps,
                    app_dropdowns,
                }
            })
            .collect()
    }
}
