use std::collections::VecDeque;
use chrono::{DateTime, Local};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Debug,
    Warn,
    Error,
    Ok,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info  => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Warn  => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Ok    => write!(f, "ok"),
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
    /// Broadcast channel sender — SSE subscribers receive every new log message.
    pub log_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new() -> Self {
        let stack_name = std::env::var("STACK_NAME").unwrap_or_else(|_| "unknown".to_string());
        let stack_ip = std::env::var("STACK_IP").unwrap_or_else(|_| "—".to_string());
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
            logs: VecDeque::with_capacity(500),
            is_syncing: false,
            sync_requested: false,
            backup_paused: false,
            log_tx,
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
            ts, level, self.stack_name,
            msg.replace('"', "'")
        );
        println!("{}", logfmt);

        let entry = LogEntry {
            timestamp: chrono::Local::now(),
            level,
            msg: msg.clone(),
        };
        self.logs.push_back(entry);
        if self.logs.len() > 500 {
            self.logs.pop_front();
        }
        // Broadcast to any connected SSE clients (ignore if no subscribers)
        let _ = self.log_tx.send(logfmt);
    }
}
