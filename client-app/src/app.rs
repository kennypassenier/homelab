//! Application state — Tab enum, App struct, and all state-management helpers.

use std::{
    collections::VecDeque,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use rand::{Rng, SeedableRng, rngs::SmallRng};

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
            Tab::Logs => "Logs",
        }
    }
}

/// Each log source displayed in the Logs tab legend, with its display colour.
/// Add new stacks here as the homelab grows.
pub const LOG_SOURCES: &[(&str, ratatui::style::Color)] = &[
    ("lxc-cloudflared", ratatui::style::Color::Blue),
    ("lxc-downloader", ratatui::style::Color::Magenta),
    ("lxc-gateway", ratatui::style::Color::Yellow),
    ("lxc-media", ratatui::style::Color::Cyan),
    ("lxc-monitoring", ratatui::style::Color::Green),
    ("lxc-paperless", ratatui::style::Color::LightCyan),
    ("lxc-vikunja", ratatui::style::Color::LightMagenta),
    ("HOST", ratatui::style::Color::White),
    ("CLIENT", ratatui::style::Color::Cyan),
];

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
}

/// Cyclic mock telemetry entries used until a live WebSocket stream is connected.
/// Each tuple is (source, level, message).
const MOCK_SEQUENCE: &[(&str, &str, &str)] = &[
    (
        "lxc-cloudflared",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-cloudflared", "INFO", "[git] Already up to date"),
    (
        "lxc-cloudflared",
        "INFO",
        "[docker] cloudflared: Image up to date",
    ),
    (
        "lxc-downloader",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-downloader", "INFO", "[git] Already up to date"),
    (
        "lxc-downloader",
        "INFO",
        "[docker] qbittorrent: Image up to date",
    ),
    (
        "lxc-downloader",
        "INFO",
        "[promtail] Shipping 17 log lines to Loki",
    ),
    (
        "lxc-gateway",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-gateway", "INFO", "[git] Already up to date"),
    ("lxc-gateway", "INFO", "[docker] crowdsec: Image up to date"),
    (
        "lxc-gateway",
        "WARN",
        "[crowdsec] Decision added: ban 203.0.113.42 (SSH bruteforce, 48 attempts)",
    ),
    (
        "lxc-gateway",
        "INFO",
        "[docker] nginx-proxy-manager: Image up to date",
    ),
    (
        "lxc-gateway",
        "INFO",
        "[promtail] Shipping 31 log lines to Loki",
    ),
    (
        "lxc-media",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-media", "INFO", "[git] Fast-forward: 1 new commit"),
    ("lxc-media", "INFO", "[pre-sync] Running pre-sync.sh..."),
    ("lxc-media", "INFO", "[docker] bazarr: Pulling newer image"),
    ("lxc-media", "INFO", "[docker] bazarr: Container recreated"),
    ("lxc-media", "INFO", "[docker] bazarr: Started"),
    ("lxc-media", "INFO", "[docker] jellyfin: Image up to date"),
    (
        "lxc-media",
        "INFO",
        "[promtail] Shipping 52 log lines to Loki",
    ),
    (
        "lxc-monitoring",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-monitoring", "INFO", "[git] Already up to date"),
    (
        "lxc-monitoring",
        "INFO",
        "[loki] Received 100 lines from 4 sources",
    ),
    (
        "lxc-monitoring",
        "INFO",
        "[grafana] Dashboard scraped: uptime-kuma",
    ),
    (
        "lxc-monitoring",
        "INFO",
        "[promtail] Shipping 8 log lines to Loki",
    ),
    (
        "lxc-paperless",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-paperless", "INFO", "[git] Already up to date"),
    (
        "lxc-vikunja",
        "INFO",
        "[node-sync] Checking for upstream changes...",
    ),
    ("lxc-vikunja", "INFO", "[git] Already up to date"),
    (
        "HOST",
        "INFO",
        "sync-host.sh: Syncing bind-mounts to lxc-media",
    ),
    ("HOST", "INFO", "sync-host.sh: Sync complete"),
    (
        "lxc-media",
        "INFO",
        "[watchtower] Checking all images for updates...",
    ),
    (
        "lxc-media",
        "INFO",
        "[watchtower] All containers up to date",
    ),
    (
        "lxc-downloader",
        "INFO",
        "[qbittorrent] Torrent completed: ubuntu-24.04.iso (4.6 GB)",
    ),
    (
        "lxc-gateway",
        "ERROR",
        "[nginx-proxy-manager] Upstream unreachable: lxc-vikunja:3456 (timeout)",
    ),
    (
        "lxc-gateway",
        "WARN",
        "[crowdsec] 5 new IPs banned this cycle",
    ),
    (
        "CLIENT",
        "INFO",
        "Git push triggered: feat(media): add bazarr",
    ),
    (
        "lxc-media",
        "INFO",
        "[node-sync] Fast-forward: 1 new commit",
    ),
    ("lxc-media", "INFO", "[pre-sync] Running pre-sync.sh..."),
    ("lxc-media", "INFO", "[docker] bazarr: Pulling image"),
    (
        "lxc-media",
        "INFO",
        "[docker] bazarr: Container created and started",
    ),
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
    /// Horizontal scroll offset for the Sources legend (index of first visible source).
    pub log_source_scroll: usize,
    /// Which log levels are visible (default: All).
    pub log_level_filter: LogLevelFilter,
    /// Index into MOCK_SEQUENCE, advances on every tick.
    log_tick: usize,
    /// Disable synthetic log playback once real LXC telemetry arrives.
    pub live_logs_seen: bool,

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
    /// When `true`, the main loop will look up the LXC IP and POST /api/sync.
    pub sync_pending: bool,
    /// The stack name to sync (set alongside `sync_pending = true`).
    pub sync_stack: String,
    /// FIFO queue for batch stack sync actions (deploy/update all active stacks).
    pub sync_queue: VecDeque<String>,
    /// Human-readable status line shown at the bottom of the Scaffolding tab.
    pub sync_status: String,
    /// Backup schedule policy edited in Backups tab.
    pub backup_schedule: BackupSchedule,
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
            log_source_scroll: 0,
            log_level_filter: LogLevelFilter::All,
            log_tick: 0,
            live_logs_seen: false,
            pulse_phase: 0.0,
            ticker_offset: 0,
            ticker_content,
            lxc_cpu,
            lxc_ram,
            host_selected: 0,
            rng,
            sync_pending: false,
            sync_stack: String::new(),
            sync_queue: VecDeque::new(),
            sync_status: "Idle".to_string(),
            backup_schedule: BackupSchedule::load_or_default(),
            backup_status: "Backup policy loaded".to_string(),
            restore_pending: false,
            restore_stack: String::new(),
            restore_queue: VecDeque::new(),
            restore_backup_id: std::env::var("BACKUP_ID_DEFAULT")
                .unwrap_or_else(|_| "latest".to_string()),
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

    /// Pushes a real log entry into the ring buffer (used by background tasks).
    pub fn push_log(&mut self, source: &str, level: &str, message: &str) {
        self.logs.push(LogLine {
            time: current_time_str(),
            source: source.to_string(),
            level: level.to_string(),
            message: message.to_string(),
        });
        if self.logs.len() > 500 {
            self.logs.remove(0);
        }
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

    /// Adds the next mock log entry to the ring buffer.
    ///
    /// Once the buffer exceeds 500 lines the oldest entry is dropped.
    /// Auto-scrolling is only applied when the user has not manually scrolled up
    /// (i.e. `log_scroll == 0`).
    pub fn tick_logs(&mut self) {
        if self.live_logs_seen {
            return;
        }
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

    pub fn mark_live_logs_seen(&mut self) {
        self.live_logs_seen = true;
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
