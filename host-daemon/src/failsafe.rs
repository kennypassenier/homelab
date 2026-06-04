use std::fs;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::self_update;

const DEFAULT_FAILSAFE_INTERVAL_SECS: u64 = 3600;
const DEFAULT_HEARTBEAT_TTL_SECS: u64 = 180;
const DEFAULT_HEARTBEAT_FILE: &str = "/tmp/homelab-client-heartbeat.ts";

pub fn start_failsafe_enforcer(status_tx: Sender<String>) {
    thread::spawn(move || {
        let mut last_window = Instant::now();

        loop {
            let interval_secs = env_u64(
                "FAILSAFE_SYNC_INTERVAL_SECS",
                DEFAULT_FAILSAFE_INTERVAL_SECS,
            )
            .max(60);
            let heartbeat_ttl_secs =
                env_u64("HEARTBEAT_TTL_SECS", DEFAULT_HEARTBEAT_TTL_SECS).max(30);
            let heartbeat_file = std::env::var("HOST_HEARTBEAT_FILE")
                .unwrap_or_else(|_| DEFAULT_HEARTBEAT_FILE.to_string());

            if last_window.elapsed().as_secs() >= interval_secs {
                match heartbeat_age_secs(&heartbeat_file) {
                    Some(age) if age <= heartbeat_ttl_secs => {
                        let _ = status_tx.send(format!(
                            "[failsafe] window skipped: heartbeat fresh (age={}s ttl={}s)",
                            age, heartbeat_ttl_secs
                        ));
                    }
                    Some(age) => {
                        let _ = status_tx.send(format!(
                            "[failsafe] heartbeat stale (age={}s ttl={}s) -> checking self-update",
                            age, heartbeat_ttl_secs
                        ));
                        match self_update::check_and_apply_update() {
                            Ok(msg) => {
                                let _ = status_tx.send(format!("[failsafe] {}", msg));
                            }
                            Err(err) => {
                                let _ = status_tx
                                    .send(format!("[failsafe] self-update failed: {}", err));
                            }
                        }
                    }
                    None => {
                        let _ = status_tx.send(
                            "[failsafe] no heartbeat file -> checking self-update".to_string(),
                        );
                        match self_update::check_and_apply_update() {
                            Ok(msg) => {
                                let _ = status_tx.send(format!("[failsafe] {}", msg));
                            }
                            Err(err) => {
                                let _ = status_tx
                                    .send(format!("[failsafe] self-update failed: {}", err));
                            }
                        }
                    }
                }

                last_window = Instant::now();
            }

            thread::sleep(Duration::from_secs(30));
        }
    });
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn heartbeat_age_secs(path: &str) -> Option<u64> {
    let raw = fs::read_to_string(path).ok()?;
    let ts = raw.trim().parse::<u64>().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

    Some(now.saturating_sub(ts))
}
