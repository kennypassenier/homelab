use std::os::unix::fs::MetadataExt;
use std::sync::{Arc, Mutex};
use crate::app::{AppState, LogLevel, MountStatus};

pub async fn run_checker(state: Arc<Mutex<AppState>>) {
    loop {
        check_mounts(state.clone());
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

fn check_mounts(state: Arc<Mutex<AppState>>) {
    let (docker_path, config_path) = {
        let s = state.lock().unwrap();
        (s.mounts.docker_path.clone(), s.mounts.config_path.clone())
    };

    // Get the root device as baseline — a path that has the SAME dev as root is NOT mounted
    let root_dev = std::fs::metadata("/").map(|m| m.dev()).unwrap_or(0);

    let (docker_ok, docker_dev) = match std::fs::metadata(&docker_path) {
        Ok(m) => {
            let dev = m.dev();
            // If dev differs from root, it's a real bind mount
            (dev != root_dev, format!("0x{:04x}", dev))
        }
        Err(_) => (false, "\u{2014}".to_string()),
    };

    let (config_ok, config_dev) = match std::fs::metadata(&config_path) {
        Ok(m) => {
            let dev = m.dev();
            (dev != root_dev, format!("0x{:04x}", dev))
        }
        Err(_) => (false, "\u{2014}".to_string()),
    };

    let status = MountStatus {
        docker_ok,
        config_ok,
        docker_path,
        config_path,
        docker_dev,
        config_dev,
    };

    let mut s = state.lock().unwrap();
    if !docker_ok || !config_ok {
        s.add_log(
            LogLevel::Warn,
            "Mount validation: one or more bind mounts missing \u{2014} containers may lack persistent storage".to_string(),
        );
    }
    s.mounts = status;
}
