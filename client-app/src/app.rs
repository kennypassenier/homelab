//! Application state — Tab enum, App struct, and all state-management helpers.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::{Rng, SeedableRng, rngs::SmallRng};
use serde::Deserialize;

use crate::backup_schedule::BackupSchedule;
use crate::blast_radius::ActiveModal;
use crate::theme::Theme;

/// Represents the four main navigation tabs.
#[derive(Copy, Clone, Debug)]
pub enum Tab {
    Dashboard,
    Scaffolding,
    Backups,
    HostManagement,
    Update,
    Logs,
}

impl Tab {
    /// Returns every tab in display order.
    pub fn all() -> &'static [Tab] {
        &[
            Tab::Dashboard,
            Tab::Scaffolding,
            Tab::Backups,
            Tab::HostManagement,
            Tab::Update,
            Tab::Logs,
        ]
    }

    /// Human-readable tab label used in the tab bar.
    pub fn title(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Scaffolding => "Scaffolding",
            Tab::Backups => "Backups",
            Tab::HostManagement => "Host Management",
            Tab::Update => "Update",
            Tab::Logs => "Logs",
        }
    }
}

/// Per-source colours — cycled through for all `lxc-*` sources by index.
const SOURCE_COLORS: &[ratatui::style::Color] = &[
    ratatui::style::Color::Blue,
    ratatui::style::Color::Magenta,
    ratatui::style::Color::Yellow,
    ratatui::style::Color::LightCyan,
    ratatui::style::Color::Green,
    ratatui::style::Color::LightMagenta,
    ratatui::style::Color::LightBlue,
    ratatui::style::Color::LightGreen,
    ratatui::style::Color::LightYellow,
    ratatui::style::Color::Rgb(255, 128, 0),
];

/// Keep a short trail of emitted CLIENT signatures to detect repeated patterns.
const CLIENT_FOLD_TRAIL_LIMIT: usize = 24;

/// Which log levels are visible in the Logs tab.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum LogLevelFilter {
    All,
    Info,
    Warn,
    Error,
}

impl LogLevelFilter {
    /// Cycle to the next filter in order: All → Info → Warn → Error → All.
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Info,
            Self::Info => Self::Warn,
            Self::Warn => Self::Error,
            Self::Error => Self::All,
        }
    }

    /// Label shown in the level filter block.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Returns true if a line with the given level string passes this filter.
    pub fn matches(self, level: &str) -> bool {
        match self {
            Self::All => true,
            Self::Info => level == "INFO",
            Self::Warn => level == "WARN",
            Self::Error => level == "ERROR",
        }
    }
}

/// A single telemetry line stored in the Logs tab ring buffer.
pub struct LogLine {
    pub time: String,
    pub source: String,
    pub level: String,
    pub message: String,
    /// Timestamp when this entry was created (used for age-based retention).
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HostLxcRuntime {
    pub vmid: u32,
    pub status: String,
    pub name: String,
    pub cpu_pct: u8,
    pub ram_pct: u8,
    pub uptime_secs: u64,
}

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
    /// Horizontal character offset for long log messages.
    pub log_hscroll: usize,
    /// When true, log messages are rendered in wrapped mode instead of horizontal pan mode.
    pub log_wrap_mode: bool,
    /// Horizontal scroll offset for the Sources legend (index of first visible source).
    pub log_source_scroll: usize,
    /// Currently selected source in the Logs source legend.
    pub log_source_selected: usize,
    /// When enabled, the Logs tab only renders lines from the selected source.
    pub log_focus_mode: bool,
    /// Which log levels are visible (default: All).
    pub log_level_filter: LogLevelFilter,
    /// True when live log telemetry has been observed.
    pub live_logs_seen: bool,
    /// Currently connected LXC stacks (source names are rendered as `lxc-<stack>`).
    connected_lxc_stacks: Vec<String>,
    /// Folded CLIENT pattern currently being suppressed.
    client_fold_pattern: Option<Vec<String>>,
    /// Next expected item index inside `client_fold_pattern`.
    client_fold_next_index: usize,
    /// Number of suppressed CLIENT lines while folding is active.
    client_fold_suppressed_lines: usize,
    /// Recently emitted CLIENT signatures (level|message), used to detect 1-3 line loops.
    client_recent_emitted: VecDeque<String>,

    // ── Animation state (driven by anim_tick at 33 ms) ─────────────────────
    /// Sinusoidal phase (0..2π) for pulse/breathing effects on selected items.
    pub pulse_phase: f32,
    /// Byte offset into `ticker_content` for the scrolling bottom status bar.
    pub ticker_offset: usize,
    /// Pre-built looping ASCII ticker string shown at the bottom of the screen.
    pub ticker_content: String,
    /// CPU load sparkline ring buffers — one `VecDeque` per stack, same order as `stacks`.
    pub lxc_cpu: Vec<VecDeque<u64>>,
    /// RAM usage sparkline ring buffers — one `VecDeque` per stack.
    pub lxc_ram: Vec<VecDeque<u64>>,
    /// Currently highlighted LXC row index in the Host Management tab.
    pub host_selected: usize,
    /// RNG seeded once at startup — only for animations, never for security.
    rng: SmallRng,
    /// When `true`, the main loop will ask HOST to run a provisioning cycle before syncing.
    pub provision_pending: bool,
    /// When `true`, the main loop will look up the LXC IP and POST /api/sync.
    pub sync_pending: bool,
    /// The stack name to sync (set alongside `sync_pending = true`).
    pub sync_stack: String,
    /// FIFO queue for batch stack sync actions (deploy/update all active stacks).
    pub sync_queue: VecDeque<String>,
    /// Human-readable status line shown at the bottom of the Scaffolding tab.
    pub sync_status: String,
    /// When true, main loop dispatches a HOST request to destroy one stack LXC.
    pub destroy_stack_pending: bool,
    /// Stack currently targeted for HOST-side container destroy.
    pub destroy_stack: String,
    /// Backup schedule policy edited in Backups tab.
    pub backup_schedule: BackupSchedule,
    /// Update operation in progress (stack name or "HOST").
    pub update_in_progress: Option<String>,
    /// Latest update result message.
    pub update_status: String,
    /// Status line in Backups tab.
    pub backup_status: String,
    /// When true, main loop dispatches a restore API request.
    pub restore_pending: bool,
    /// Stack currently targeted for restore dispatch.
    pub restore_stack: String,
    /// Queue for batch restore operations.
    pub restore_queue: VecDeque<String>,
    /// Backup snapshot id sent to restore backend.
    pub restore_backup_id: String,
    /// Whether HOST metrics are reachable via HTTP API.
    pub host_connected: bool,
    /// Last discovered Proxmox node hostname.
    pub host_node_name: String,
    /// Last discovered Proxmox node primary IP.
    pub host_node_ip: String,
    /// Last discovered host uptime string.
    pub host_uptime: String,
    /// Last known LXC runtime rows from HOST metrics API.
    pub host_lxc_runtime: Vec<HostLxcRuntime>,
    /// Last host metrics polling error (shown when disconnected).
    pub host_last_error: String,
    /// Last observed HOST daemon version from websocket log stream.
    pub host_daemon_version: String,
    /// Latest available HOST release tag from GitHub (`host-daemon-v*`).
    pub host_latest_release: String,
    /// Wall-clock timestamp for the last HOST release metadata refresh.
    pub host_latest_checked_at: String,
    /// Last observed LXC daemon version per websocket source (`lxc-<stack>`).
    pub lxc_daemon_versions: HashMap<String, String>,
    /// LXC self-update channel (compose image reference, usually `:latest`).
    pub lxc_update_channel: String,
    /// Last manual update result summary per target (`HOST`, stack name, `UPDATING_ALL`).
    pub update_last_result: HashMap<String, String>,
    /// Wall-clock timestamp (`HH:MM:SS`) for the last update result per target.
    pub update_last_at: HashMap<String, String>,
}

impl App {
    /// Creates a freshly initialised application, loading stacks from disk.
    pub fn new() -> Self {
        let stacks = App::load_stacks();
        let stack_dropdowns = Self::build_dropdowns(&stacks);
        let ticker_content = Self::build_ticker_content();
        let mut rng = SmallRng::from_entropy();

        // Pre-fill 30 random-walk samples per stack so sparklines look populated
        // on the very first render.
        let lxc_cpu: Vec<VecDeque<u64>> = stacks
            .iter()
            .map(|_| {
                let mut d = VecDeque::new();
                let mut v: i64 = rng.gen_range(10..70);
                for _ in 0..30 {
                    v = (v + rng.gen_range(-5..=5)).clamp(2, 98);
                    d.push_back(v as u64);
                }
                d
            })
            .collect();

        let lxc_ram: Vec<VecDeque<u64>> = stacks
            .iter()
            .map(|_| {
                let mut d = VecDeque::new();
                let mut v: i64 = rng.gen_range(20..80);
                for _ in 0..30 {
                    v = (v + rng.gen_range(-3..=3)).clamp(10, 95);
                    d.push_back(v as u64);
                }
                d
            })
            .collect();

        let app = Self {
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
            log_hscroll: 0,
            log_wrap_mode: false,
            log_source_scroll: 0,
            log_source_selected: 0,
            log_focus_mode: false,
            log_level_filter: LogLevelFilter::All,
            live_logs_seen: false,
            connected_lxc_stacks: Vec::new(),
            client_fold_pattern: None,
            client_fold_next_index: 0,
            client_fold_suppressed_lines: 0,
            client_recent_emitted: VecDeque::new(),
            pulse_phase: 0.0,
            ticker_offset: 0,
            ticker_content,
            lxc_cpu,
            lxc_ram,
            host_selected: 0,
            rng,
            provision_pending: false,
            sync_pending: false,
            sync_stack: String::new(),
            sync_queue: VecDeque::new(),
            sync_status: "Idle".to_string(),
            destroy_stack_pending: false,
            destroy_stack: String::new(),
            backup_schedule: BackupSchedule::load_or_default(),
            update_in_progress: None,
            update_status: String::new(),
            backup_status: "Backup policy loaded".to_string(),
            restore_pending: false,
            restore_stack: String::new(),
            restore_queue: VecDeque::new(),
            restore_backup_id: std::env::var("BACKUP_ID_DEFAULT")
                .unwrap_or_else(|_| "latest".to_string()),
            host_connected: false,
            host_node_name: "unknown".to_string(),
            host_node_ip: "unknown".to_string(),
            host_uptime: "unknown".to_string(),
            host_lxc_runtime: Vec::new(),
            host_last_error: "not connected yet".to_string(),
            host_daemon_version: "unknown".to_string(),
            host_latest_release: "checking...".to_string(),
            host_latest_checked_at: "-".to_string(),
            lxc_daemon_versions: HashMap::new(),
            lxc_update_channel: std::env::var("LXC_DAEMON_IMAGE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "ghcr.io/kennypassenier/homelab-lxc-daemon:latest".to_string()),
            update_last_result: HashMap::new(),
            update_last_at: HashMap::new(),
        };
        app
    }

    /// Finds the project root by searching for .git directory.
    fn find_project_root() -> std::path::PathBuf {
        let mut current = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        for _ in 0..10 {
            if current.join(".git").exists() {
                return current;
            }
            if !current.pop() {
                break;
            }
        }
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    /// Scans `stacks/` (at project root) for infrastructure stacks.
    pub fn load_stacks() -> Vec<String> {
        let mut result = Vec::new();
        let root = Self::find_project_root();
        let stacks_dir = root.join("stacks");
        if let Ok(entries) = fs::read_dir(&stacks_dir) {
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

    /// Pushes a real log entry into the ring buffer (used by background tasks).
    ///
    /// HOST and LXC now cap their own replay histories, so the client can keep the
    /// full session stream without trimming older entries here.
    pub fn push_log(&mut self, source: &str, level: &str, message: &str) {
        let now = SystemTime::now();

        // Zero-buffer pass-through for non-CLIENT sources.
        if source != "CLIENT" {
            self.flush_client_fold_summary_if_needed();
            self.push_log_raw(source, level, message, now);
            return;
        }

        let signature = format!("{}|{}", level, message);

        // If we're currently folding a CLIENT pattern, keep suppressing while it matches.
        if self.client_fold_pattern.is_some() {
            let expected = {
                let pattern = self.client_fold_pattern.as_ref().unwrap();
                pattern[self.client_fold_next_index].clone()
            };
            if signature == expected {
                self.client_fold_suppressed_lines += 1;
                let pattern_len = self
                    .client_fold_pattern
                    .as_ref()
                    .map(|p| p.len())
                    .unwrap_or(1)
                    .max(1);
                self.client_fold_next_index = (self.client_fold_next_index + 1) % pattern_len;
                return;
            }

            // Pattern broke: emit one summary line, then continue with the current line.
            self.flush_client_fold_summary_if_needed();
        }

        // Start folding only after the first instance has already been printed.
        if let Some((pattern, next_index)) = self.detect_client_repeat_start(&signature) {
            self.client_fold_pattern = Some(pattern);
            self.client_fold_next_index = next_index;
            self.client_fold_suppressed_lines = 1;
            return;
        }

        // Default: immediate pass-through.
        self.push_log_raw(source, level, message, now);
        self.client_recent_emitted.push_back(signature);
        while self.client_recent_emitted.len() > CLIENT_FOLD_TRAIL_LIMIT {
            self.client_recent_emitted.pop_front();
        }
    }

    fn push_log_raw(&mut self, source: &str, level: &str, message: &str, now: SystemTime) {
        self.logs.push(LogLine {
            time: current_time_str(),
            source: source.to_string(),
            level: level.to_string(),
            message: message.to_string(),
            created_at: now,
        });
    }

    fn detect_client_repeat_start(&self, signature: &str) -> Option<(Vec<String>, usize)> {
        if let Some(last) = self.client_recent_emitted.back() {
            if last == signature {
                return Some((vec![signature.to_string()], 0));
            }
        }

        let recent: Vec<String> = self.client_recent_emitted.iter().cloned().collect();
        for pattern_len in [2usize, 3usize] {
            if recent.len() < pattern_len * 2 {
                continue;
            }
            let start = recent.len() - (pattern_len * 2);
            let first = &recent[start..start + pattern_len];
            let second = &recent[start + pattern_len..start + pattern_len * 2];
            if first == second && signature == first[0] {
                let pattern: Vec<String> = first.to_vec();
                return Some((pattern, 1 % pattern_len));
            }
        }

        None
    }

    fn flush_client_fold_summary_if_needed(&mut self) {
        if self.client_fold_suppressed_lines == 0 {
            self.client_fold_pattern = None;
            self.client_fold_next_index = 0;
            return;
        }

        let pattern_len = self
            .client_fold_pattern
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(1)
            .max(1);
        let repeats = (self.client_fold_suppressed_lines / pattern_len).max(1);
        let summary = format!("[ ↳ Above CLIENT pattern repeated {} times ]", repeats);
        self.push_log_raw("CLIENT", "INFO", &summary, SystemTime::now());

        self.client_fold_pattern = None;
        self.client_fold_next_index = 0;
        self.client_fold_suppressed_lines = 0;
    }

    /// Builds a canonical logfmt message used across client-side events.
    pub fn logfmt(
        component: &str,
        level: &str,
        stack: Option<&str>,
        phase: Option<&str>,
        msg: &str,
        error: Option<&str>,
    ) -> String {
        let mut parts = Vec::new();
        parts.push(format!("component={}", component));
        parts.push(format!("level={}", level.to_lowercase()));
        if let Some(stack) = stack {
            parts.push(format!("stack={}", stack));
        }
        if let Some(phase) = phase {
            parts.push(format!("phase={}", phase));
        }
        parts.push(format!("msg=\"{}\"", msg.replace('"', "'")));
        if let Some(err) = error {
            parts.push(format!("error=\"{}\"", err.replace('"', "'")));
        }
        parts.join(" ")
    }

    /// Emits a structured CLIENT log line to the in-memory log buffer.
    pub fn push_client_logfmt(
        &mut self,
        level: &str,
        stack: Option<&str>,
        phase: Option<&str>,
        msg: &str,
        error: Option<&str>,
    ) {
        let line = Self::logfmt("client", level, stack, phase, msg, error);
        self.push_log("CLIENT", &level.to_uppercase(), &line);
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

    pub fn mark_live_logs_seen(&mut self) {
        self.live_logs_seen = true;
    }

    pub fn focused_source(&self) -> Option<String> {
        if !self.log_focus_mode {
            return None;
        }
        self.log_sources()
            .get(self.log_source_selected)
            .map(|(name, _)| name.clone())
    }

    pub fn set_connected_lxc_stacks<I>(&mut self, stacks: I)
    where
        I: IntoIterator<Item = String>,
    {
        let mut unique = HashSet::new();
        let mut values: Vec<String> = stacks
            .into_iter()
            .filter(|v| !v.trim().is_empty())
            .filter(|v| unique.insert(v.clone()))
            .collect();
        values.sort();
        self.connected_lxc_stacks = values;

        let count = self.log_sources().len();
        if count == 0 {
            self.log_source_selected = 0;
            self.log_source_scroll = 0;
            return;
        }
        if self.log_source_selected >= count {
            self.log_source_selected = count - 1;
        }
        self.log_source_scroll = self.log_source_scroll.min(self.log_source_selected);
    }

    /// Dynamic log source list: connected LXC workers, then HOST (when connected), then CLIENT.
    pub fn log_sources(&self) -> Vec<(String, ratatui::style::Color)> {
        let mut sources: Vec<(String, ratatui::style::Color)> = self
            .connected_lxc_stacks
            .iter()
            .enumerate()
            .map(|(i, stack)| {
                (
                    format!("lxc-{}", stack),
                    SOURCE_COLORS[i % SOURCE_COLORS.len()],
                )
            })
            .collect();
        if self.host_connected {
            sources.push(("HOST".to_string(), ratatui::style::Color::White));
        }
        sources.push(("CLIENT".to_string(), ratatui::style::Color::Cyan));
        sources
    }

    pub fn update_host_runtime(
        &mut self,
        connected: bool,
        node_name: Option<String>,
        node_ip: Option<String>,
        uptime: Option<String>,
        lxc_runtime: Vec<HostLxcRuntime>,
        error: Option<String>,
    ) {
        self.host_connected = connected;
        if let Some(name) = node_name {
            self.host_node_name = name;
        }
        if let Some(ip) = node_ip {
            self.host_node_ip = ip;
        }
        if let Some(uptime) = uptime {
            self.host_uptime = uptime;
        }
        self.host_lxc_runtime = lxc_runtime;
        self.host_last_error = error.unwrap_or_default();
    }

    /// Stores per-target update outcome metadata for Update tab cards.
    pub fn record_update_result(&mut self, target: &str, ok: bool, msg: &str) {
        let status = if ok { "success" } else { "failed" };
        let summary = format!("{}: {}", status, msg);
        let compact = if summary.chars().count() > 110 {
            let mut clipped: String = summary.chars().take(107).collect();
            clipped.push_str("...");
            clipped
        } else {
            summary
        };

        self.update_last_result.insert(target.to_string(), compact);
        self.update_last_at
            .insert(target.to_string(), current_time_str());
    }

    // ── private helpers ─────────────────────────────────────────────────────

    /// Advances all animation state by one tick (called at ~30 FPS).
    pub fn tick_anim(&mut self) {
        use std::f32::consts::TAU;

        // Sinusoidal pulse phase for breathing highlights.
        self.pulse_phase = (self.pulse_phase + 0.08) % TAU;

        // Scroll the bottom ticker one char per tick.
        let ticker_len = self.ticker_content.chars().count();
        if ticker_len > 0 {
            self.ticker_offset = (self.ticker_offset + 1) % ticker_len;
        }

        // Grow sparkline deques if new stacks were added since last tick.
        while self.lxc_cpu.len() < self.stacks.len() {
            let mut d = VecDeque::new();
            d.push_back(self.rng.gen_range(10u64..70));
            self.lxc_cpu.push(d);
        }
        while self.lxc_ram.len() < self.stacks.len() {
            let mut d = VecDeque::new();
            d.push_back(self.rng.gen_range(20u64..80));
            self.lxc_ram.push(d);
        }

        // Random walk per LXC CPU load.
        for cpu in &mut self.lxc_cpu {
            let last = cpu.back().copied().unwrap_or(30) as i64;
            let next = (last + self.rng.gen_range(-5..=5)).clamp(2, 98) as u64;
            cpu.push_back(next);
            if cpu.len() > 60 {
                cpu.pop_front();
            }
        }

        // Random walk per LXC RAM usage.
        for ram in &mut self.lxc_ram {
            let last = ram.back().copied().unwrap_or(40) as i64;
            let next = (last + self.rng.gen_range(-3..=3)).clamp(10, 95) as u64;
            ram.push_back(next);
            if ram.len() > 60 {
                ram.pop_front();
            }
        }
    }

    fn build_ticker_content() -> String {
        // A looping telemetry string. Intentionally long so it doesn't visibly repeat
        // within a normal session.
        String::from(
            "  \u{25cf} 0xFF4A2F :: ETH0 TX=1.2MB/s RX=430KB/s \
             :: pve-01 [ONLINE] :: GITOPS [ACTIVE] :: node-sync [RUN] \
             :: docker [UP] :: crowdsec [ARMED] :: uptime:47d12h \
             :: CRC_OK :: MESH_STABLE :: SOPS_OK :: AGE_KEY [VALID] \
             :: 0xDE:AD:BE:EF:00:01 :: lat=0.4ms :: loki [OK] \
             :: grafana [OK] :: 6 LXC [ONLINE] :: PUSH_READY \
             :: SHA256:aB3x... :: 192.168.1.0/24 [STABLE] ::  ",
        )
    }

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
