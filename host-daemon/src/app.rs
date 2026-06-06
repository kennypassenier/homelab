#[derive(Debug, Clone)]
pub struct LxcNode {
    pub id: u32,
    pub name: String,
    /// Resolved IP address used for HTTP API calls (pause/resume).
    /// Falls back to the `LXC_<STACKNAME_UPPER>_IP` env var, then the default.
    pub ip: String,
    pub status: String,
    pub cpu: f64,
    pub ram: Option<String>,
}

impl LxcNode {
    fn resolve_ip(stack_name: &str, default: &str) -> String {
        let key = format!("LXC_{}_IP", stack_name.replace('-', "_").to_uppercase());
        std::env::var(&key).unwrap_or_else(|_| default.to_string())
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BackupStack {
    pub repo: String,
    pub schedule: String,
    pub last_run: String,
    pub duration: String,
    pub size_raw: String,
    pub size_dedup: String,
    pub snapshots: u32,
}

#[derive(Clone, Debug)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Ok,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Ok => write!(f, "OK"),
        }
    }
}

use crate::api::HostMetrics;

const DEFAULT_LOG_HISTORY_LIMIT: usize = 500;

pub struct App {
    pub tab: usize,
    /// Live status lines from the backup orchestrator thread.
    pub backup_status: Vec<String>,
    /// True while a backup run is in progress.
    pub backup_running: bool,
    /// Broadcast channel for streaming logs to WebSocket clients.
    pub log_tx: tokio::sync::broadcast::Sender<String>,
    /// Current HOST metrics exposed via /api/metrics endpoint.
    pub current_metrics: HostMetrics,
    /// Process start timestamp for uptime calculation.
    pub started_at: std::time::Instant,
    /// Max retained log lines kept in memory for websocket replay.
    log_history_limit: usize,
}

impl App {
    pub fn new() -> Self {
        let (log_tx, _) = tokio::sync::broadcast::channel(128);
        Self {
            tab: 0,
            backup_status: Vec::new(),
            backup_running: false,
            log_tx,
            current_metrics: HostMetrics {
                hostname: "proxmox".to_string(),
                ip: "10.10.5.250".to_string(),
                uptime_secs: 0,
                lxc_runtime: Vec::new(),
            },
            started_at: std::time::Instant::now(),
            log_history_limit: parse_log_history_limit(),
        }
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let log_line = format!("[{}] {}", level, message);
        self.backup_status.push(log_line.clone());
        if self.backup_status.len() > self.log_history_limit {
            self.backup_status.remove(0);
        }
        let _ = self.log_tx.send(log_line);
    }

    pub fn lxc_nodes(&self) -> Vec<LxcNode> {
        vec![
            LxcNode {
                id: 101,
                name: "lxc-cloudflared".to_string(),
                ip: LxcNode::resolve_ip("cloudflared", "10.0.1.101"),
                status: "RUN".to_string(),
                cpu: 3.0,
                ram: Some("128/512 MB".to_string()),
            },
            LxcNode {
                id: 102,
                name: "lxc-downloader".to_string(),
                ip: LxcNode::resolve_ip("downloader", "10.0.1.102"),
                status: "RUN".to_string(),
                cpu: 8.0,
                ram: Some("210/512 MB".to_string()),
            },
            LxcNode {
                id: 103,
                name: "lxc-gateway".to_string(),
                ip: LxcNode::resolve_ip("gateway", "10.0.1.103"),
                status: "RUN".to_string(),
                cpu: 22.0,
                ram: Some("380/1024 MB".to_string()),
            },
            LxcNode {
                id: 104,
                name: "lxc-media".to_string(),
                ip: LxcNode::resolve_ip("media", "10.0.1.104"),
                status: "STOP".to_string(),
                cpu: 0.0,
                ram: Some("--/1024 MB".to_string()),
            },
            LxcNode {
                id: 105,
                name: "lxc-monitoring".to_string(),
                ip: LxcNode::resolve_ip("monitoring", "10.0.1.105"),
                status: "RUN".to_string(),
                cpu: 11.0,
                ram: Some("290/512 MB".to_string()),
            },
            LxcNode {
                id: 106,
                name: "lxc-paperless".to_string(),
                ip: LxcNode::resolve_ip("paperless", "10.0.1.106"),
                status: "RUN".to_string(),
                cpu: 17.0,
                ram: Some("640/1024 MB".to_string()),
            },
        ]
    }

    pub fn backup_stack(&self) -> BackupStack {
        BackupStack {
            repo: "/mnt/backup/restic".to_string(),
            schedule: "daily @ 03:00".to_string(),
            last_run: "2026-05-26 03:00".to_string(),
            duration: "4m 32s".to_string(),
            size_raw: "87.4 GB (raw)".to_string(),
            size_dedup: "14.2 GB (dedup, 83.7% savings)".to_string(),
            snapshots: 42,
        }
    }
}

fn parse_log_history_limit() -> usize {
    std::env::var("HOST_LOG_HISTORY_MAX")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|v| v.clamp(50, 10_000))
        .unwrap_or(DEFAULT_LOG_HISTORY_LIMIT)
}
