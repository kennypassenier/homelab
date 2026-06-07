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

use std::{
    collections::VecDeque,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::api::HostMetrics;

const DEFAULT_LOG_HISTORY_LIMIT: usize = 10_000;
const DEFAULT_LOG_HISTORY_AGE_SECS: u64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BackupStatusPriority {
    Info,
    Warn,
    Ok,
    Error,
}

impl BackupStatusPriority {
    fn weight(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warn => 1,
            Self::Ok => 2,
            Self::Error => 3,
        }
    }
}

impl From<LogLevel> for BackupStatusPriority {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Info => Self::Info,
            LogLevel::Warn => Self::Warn,
            LogLevel::Ok => Self::Ok,
            LogLevel::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupStatusLine {
    pub message: String,
    pub created_at: SystemTime,
    priority: BackupStatusPriority,
}

pub struct App {
    pub tab: usize,
    /// Live status lines from the backup orchestrator thread.
    pub backup_status: VecDeque<BackupStatusLine>,
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
    /// Age threshold for deciding which entries count toward the old-line cap.
    log_history_age_secs: u64,
}

impl App {
    pub fn new() -> Self {
        let (log_tx, _) = tokio::sync::broadcast::channel(128);
        Self {
            tab: 0,
            backup_status: VecDeque::new(),
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
            log_history_age_secs: parse_log_history_age_secs(),
        }
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let log_line = format!("[{}] {}", level, message);
        self.push_status_line_internal(log_line.clone(), level.into());
        let _ = self.log_tx.send(log_line);
    }

    pub fn push_status_line(&mut self, line: String) {
        self.push_status_line_internal(line, BackupStatusPriority::Info);
    }

    fn push_status_line_internal(&mut self, line: String, priority: BackupStatusPriority) {
        self.backup_status.push_back(BackupStatusLine {
            message: line,
            created_at: SystemTime::now(),
            priority,
        });
        self.trim_backup_status();
    }

    fn trim_backup_status(&mut self) {
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(self.log_history_age_secs))
            .unwrap_or(UNIX_EPOCH);

        let old_logs_count = self
            .backup_status
            .iter()
            .filter(|log| log.created_at < cutoff)
            .count();

        if old_logs_count <= self.log_history_limit {
            return;
        }

        let mut excess = old_logs_count - self.log_history_limit;
        while excess > 0 {
            let candidate_index = self
                .backup_status
                .iter()
                .enumerate()
                .filter(|(_, log)| log.created_at < cutoff)
                .min_by_key(|(index, log)| (log.priority.weight(), *index))
                .map(|(index, _)| index);

            let Some(index) = candidate_index else {
                break;
            };

            self.backup_status.remove(index);
            excess -= 1;
        }
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
        .map(|v| v.clamp(50, DEFAULT_LOG_HISTORY_LIMIT))
        .unwrap_or(DEFAULT_LOG_HISTORY_LIMIT)
}

fn parse_log_history_age_secs() -> u64 {
    std::env::var("LOG_HISTORY_AGE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_LOG_HISTORY_AGE_SECS)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{App, BackupStatusLine, BackupStatusPriority};

    #[test]
    fn status_history_is_bounded() {
        let mut app = App::new();
        let old_timestamp = SystemTime::now() - Duration::from_secs(7200);

        app.backup_status.push_back(BackupStatusLine {
            message: "info-a".to_string(),
            created_at: old_timestamp,
            priority: BackupStatusPriority::Info,
        });
        app.backup_status.push_back(BackupStatusLine {
            message: "warn-a".to_string(),
            created_at: old_timestamp,
            priority: BackupStatusPriority::Warn,
        });
        app.backup_status.push_back(BackupStatusLine {
            message: "error-a".to_string(),
            created_at: old_timestamp,
            priority: BackupStatusPriority::Error,
        });

        app.trim_backup_status();

        assert_eq!(app.backup_status.len(), 3);
        assert_eq!(app.backup_status[0].message, "info-a");

        for index in 0..=10_000 {
            app.backup_status.push_back(BackupStatusLine {
                message: format!("line-{index}"),
                created_at: old_timestamp,
                priority: BackupStatusPriority::Info,
            });
        }

        app.trim_backup_status();

        assert_eq!(app.backup_status.len(), 10_000);
        assert!(
            !app.backup_status
                .iter()
                .any(|line| line.message == "info-a")
        );
        assert!(
            app.backup_status
                .iter()
                .any(|line| line.message == "warn-a")
        );
        assert!(
            app.backup_status
                .iter()
                .any(|line| line.message == "error-a")
        );
    }
}
