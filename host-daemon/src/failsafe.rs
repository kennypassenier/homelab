use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crate::liveness;
use crate::self_update;

const DEFAULT_FAILSAFE_INTERVAL_SECS: u64 = 3600;
const DEFAULT_HEARTBEAT_TTL_SECS: u64 = 180;

pub fn start_failsafe_enforcer(status_tx: Sender<String>) {
    thread::spawn(move || {
        let auto_update_enabled = std::env::var("HOST_FAILSAFE_UPDATE_ENABLED")
            .ok()
            .map(|v| {
                let normalized = v.trim().to_ascii_lowercase();
                matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(true);

        if !auto_update_enabled {
            let _ = status_tx.send(
                "[failsafe] self-update checks disabled (set HOST_FAILSAFE_UPDATE_ENABLED=1 to re-enable stale-heartbeat update checks)".to_string(),
            );
            return;
        }

        let mut last_window = Instant::now();
        let mut last_no_update_log = Instant::now() - Duration::from_secs(9999);
        let mut last_no_heartbeat_log = Instant::now() - Duration::from_secs(9999);

        loop {
            let interval_secs = env_u64(
                "FAILSAFE_SYNC_INTERVAL_SECS",
                DEFAULT_FAILSAFE_INTERVAL_SECS,
            )
            .max(60);
            let heartbeat_ttl_secs =
                env_u64("HEARTBEAT_TTL_SECS", DEFAULT_HEARTBEAT_TTL_SECS).max(30);

            if last_window.elapsed().as_secs() >= interval_secs {
                match liveness::heartbeat_age_secs() {
                    Some(age) if age <= heartbeat_ttl_secs => {
                        let _ = status_tx.send(format!(
                            "[failsafe] window skipped: heartbeat fresh (age={}s ttl={}s)",
                            age, heartbeat_ttl_secs
                        ));
                    }
                    Some(age) => {
                        if last_no_heartbeat_log.elapsed().as_secs() >= 3600 {
                            let _ = status_tx.send(format!(
                                "[failsafe] heartbeat stale (age={}s ttl={}s) -> checking self-update",
                                age, heartbeat_ttl_secs
                            ));
                            last_no_heartbeat_log = Instant::now();
                        }
                        match self_update::check_and_apply_update() {
                            Ok(msg) => {
                                if !msg.contains("No HOST update available")
                                    || last_no_update_log.elapsed().as_secs() >= 21_600
                                {
                                    let _ = status_tx.send(format!("[failsafe] {}", msg));
                                    if msg.contains("No HOST update available") {
                                        last_no_update_log = Instant::now();
                                    }
                                }
                            }
                            Err(err) => {
                                let _ = status_tx
                                    .send(format!("[failsafe] self-update failed: {}", err));
                            }
                        }
                    }
                    None => {
                        if last_no_heartbeat_log.elapsed().as_secs() >= 3600 {
                            let _ = status_tx.send(
                                "[failsafe] no client heartbeat yet -> checking self-update"
                                    .to_string(),
                            );
                            last_no_heartbeat_log = Instant::now();
                        }
                        match self_update::check_and_apply_update() {
                            Ok(msg) => {
                                if !msg.contains("No HOST update available")
                                    || last_no_update_log.elapsed().as_secs() >= 21_600
                                {
                                    let _ = status_tx.send(format!("[failsafe] {}", msg));
                                    if msg.contains("No HOST update available") {
                                        last_no_update_log = Instant::now();
                                    }
                                }
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
