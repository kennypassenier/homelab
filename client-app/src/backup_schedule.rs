use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub retention_daily: u32,
    pub retention_weekly: u32,
    pub retention_monthly: u32,
    pub notify_on_success: bool,
    pub notify_on_failure: bool,
}

impl Default for BackupSchedule {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 24 * 60,
            retention_daily: 7,
            retention_weekly: 4,
            retention_monthly: 3,
            notify_on_success: false,
            notify_on_failure: true,
        }
    }
}

impl BackupSchedule {
    pub fn load_or_default() -> Self {
        let path = schedule_path();
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };

        serde_json::from_str(&raw).unwrap_or_else(|_| Self::default())
    }

    pub fn save(&self) -> io::Result<()> {
        let path = schedule_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, raw)
    }
}

fn schedule_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("homelab")
        .join("backup-schedule.json")
}
