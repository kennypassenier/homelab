use crate::app::{AppState, LogLevel, MountStatus};
use std::sync::{Arc, Mutex};

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

    let (docker_ok, docker_dev) = check_mount_path(&docker_path, true);
    let (config_ok, config_dev) = check_mount_path(&config_path, false);

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

fn check_mount_path(path: &str, required: bool) -> (bool, String) {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return if required {
            (false, "missing-path".to_string())
        } else {
            (true, "disabled".to_string())
        };
    }

    if std::fs::metadata(trimmed).is_err() {
        return (false, "missing".to_string());
    }

    if is_mountpoint(trimmed) {
        (true, "mounted".to_string())
    } else {
        (false, "not-mounted".to_string())
    }
}

fn is_mountpoint(path: &str) -> bool {
    let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };

    mountinfo
        .lines()
        .filter_map(|line| line.split_whitespace().nth(4))
        .any(|mountpoint| mountpoint == path)
}
