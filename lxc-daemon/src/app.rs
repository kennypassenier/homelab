use chrono::{DateTime, Local};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    #[allow(dead_code)]
    Debug,
    Warn,
    Error,
    Ok,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Ok => write!(f, "ok"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub msg: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContainerInfo {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    pub uptime: String,
}

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub repo_url: String,
    pub branch: String,
    pub commit: String,
    pub sparse: String,
    pub is_synced: bool,
    pub last_sync: String,
    pub next_sync: String,
    pub lock_free: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MountStatus {
    pub docker_ok: bool,
    pub config_ok: bool,
    pub docker_path: String,
    pub config_path: String,
    pub docker_dev: String,
    pub config_dev: String,
}

#[derive(Debug, Clone, Default)]
pub struct SecretsStatus {
    pub loaded: bool,
    pub target: String,
    pub method: String,
    pub loaded_ago: String,
    pub last_run_log: Vec<(String, bool)>,
}

pub struct AppState {
    pub tab: usize,
    pub stack_name: String,
    pub stack_ip: String,
    pub uptime: String,
    pub containers: Vec<ContainerInfo>,
    pub git: GitStatus,
    pub mounts: MountStatus,
    pub secrets: SecretsStatus,
    pub logs: VecDeque<LogEntry>,
    pub is_syncing: bool,
    pub sync_requested: bool,
    pub backup_paused: bool,
    /// Unix timestamp (seconds) of last CLIENT heartbeat received.
    pub client_heartbeat_ts: Option<i64>,
    /// Broadcast channel sender — WebSocket clients receive every new log message.
    pub log_tx: broadcast::Sender<String>,
    /// Max retained log lines kept in memory for websocket replay.
    log_history_limit: usize,
}

const DEFAULT_LOG_HISTORY_LIMIT: usize = 10_000;
const DEFAULT_LOG_HISTORY_AGE_SECS: u64 = 3600;

impl AppState {
    pub fn new() -> Self {
        let stack_name = detect_stack_name();
        let stack_ip = detect_stack_ip(&stack_name);
        let target = format!("/opt/appdata/{}/.env", stack_name);
        let (log_tx, _) = broadcast::channel(512);
        Self {
            tab: 0,
            stack_name,
            stack_ip,
            uptime: "—".to_string(),
            containers: Vec::new(),
            git: GitStatus {
                next_sync: "in 30m".to_string(),
                lock_free: true,
                ..Default::default()
            },
            mounts: MountStatus {
                docker_path: "/docker".to_string(),
                config_path: "/config".to_string(),
                ..Default::default()
            },
            secrets: SecretsStatus {
                method: "Ephemeral Docker container (Fail-Closed)".to_string(),
                target,
                ..Default::default()
            },
            logs: VecDeque::with_capacity(DEFAULT_LOG_HISTORY_LIMIT),
            is_syncing: false,
            sync_requested: false,
            backup_paused: false,
            client_heartbeat_ts: None,
            log_tx,
            log_history_limit: parse_log_history_limit(),
        }
    }

    pub fn next_tab(&mut self) {
        self.tab = (self.tab + 1) % 5;
    }

    pub fn prev_tab(&mut self) {
        if self.tab > 0 {
            self.tab -= 1;
        }
    }

    pub fn add_log(&mut self, level: LogLevel, msg: String) {
        // Emit logfmt line to stdout so Promtail can scrape it from Docker logs
        let ts = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let logfmt = format!(
            "ts={} level={} stack={} msg=\"{}\"",
            ts,
            level,
            self.stack_name,
            msg.replace('"', "'")
        );
        println!("{}", logfmt);

        let entry = LogEntry {
            timestamp: chrono::Local::now(),
            level,
            msg: msg.clone(),
        };
        self.logs.push_back(entry);
        self.trim_log_history();
        // Broadcast to any connected WebSocket clients (ignore if no subscribers)
        let _ = self.log_tx.send(logfmt);
    }

    fn trim_log_history(&mut self) {
        let cutoff = chrono::Local::now() - chrono::Duration::seconds(self.log_history_age_secs() as i64);
        let old_logs_count = self.logs.iter().filter(|log| log.timestamp < cutoff).count();

        if old_logs_count > self.log_history_limit {
            let excess = old_logs_count - self.log_history_limit;
            let mut removed = 0;

            while removed < excess {
                match self.logs.front() {
                    Some(front) if front.timestamp < cutoff => {
                        self.logs.pop_front();
                        removed += 1;
                    }
                    _ => break,
                }
            }
        }
    }

    fn log_history_age_secs(&self) -> u64 {
        std::env::var("LOG_HISTORY_AGE_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_LOG_HISTORY_AGE_SECS)
    }
}

fn detect_stack_name() -> String {
    if let Ok(env_stack) = std::env::var("STACK_NAME") {
        let trimmed = env_stack.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(stack) = read_stack_name_from_daemon_config() {
        return stack;
    }

    "unknown".to_string()
}

fn detect_stack_ip(stack_name: &str) -> String {
    if let Ok(env_ip) = std::env::var("STACK_IP") {
        let trimmed = env_ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    read_reserved_ip_from_lxc_compose(stack_name).unwrap_or_else(|| "—".to_string())
}

fn parse_log_history_limit() -> usize {
    std::env::var("LXC_LOG_HISTORY_MAX")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|v| v.clamp(50, DEFAULT_LOG_HISTORY_LIMIT))
        .unwrap_or(DEFAULT_LOG_HISTORY_LIMIT)
}

fn read_stack_name_from_daemon_config() -> Option<String> {
    let config_path = Path::new("/etc/homelab/lxc-daemon.toml");
    let content = fs::read_to_string(config_path).ok()?;

    let mut in_sync_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_sync_section = trimmed == "[sync]";
            continue;
        }

        if in_sync_section && trimmed.starts_with("stack_name") {
            let (_, value) = trimmed.split_once('=')?;
            let parsed = value.trim().trim_matches('"');
            if !parsed.is_empty() {
                return Some(parsed.to_string());
            }
        }
    }

    None
}

fn read_reserved_ip_from_lxc_compose(stack_name: &str) -> Option<String> {
    if stack_name.is_empty() || stack_name == "unknown" {
        return None;
    }

    let compose_path = format!("/opt/gitops/stacks/{}/lxc-compose.yml", stack_name);
    let content = fs::read_to_string(&compose_path).ok()?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;

    yaml["network"]["reserved_ipv4"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
