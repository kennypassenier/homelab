use std::path::Path;
use std::sync::{Arc, Mutex};

mod api;
mod app;
mod docker;
mod gitops;
mod mounts;
mod restore;
mod self_update;

use app::AppState;

fn load_lxc_env() {
    let candidates = [
        std::env::var("LXC_ENV_FILE").ok(),
        Some("config/.env".to_string()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = Path::new(&candidate);
        if path.exists() {
            let _ = dotenvy::from_path(path);
            break;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_lxc_env();
    let state = Arc::new(Mutex::new(AppState::new()));
    {
        let mut s = state.lock().unwrap();
        let stack_name = s.stack_name.clone();
        let stack_ip = s.stack_ip.clone();
        s.add_log(
            app::LogLevel::Info,
            format!(
                "LXC daemon online mode=headless daemon_version={} stack={} stack_ip={}",
                env!("CARGO_PKG_VERSION"),
                stack_name,
                stack_ip
            ),
        );
    }

    tokio::spawn(api::run_server(state.clone()));
    tokio::spawn(docker::run_poller(state.clone()));
    tokio::spawn(gitops::run_checker(state.clone()));
    tokio::spawn(mounts::run_checker(state.clone()));

    // Run permanently — systemd (Restart=always + StartLimitIntervalSec=0) handles
    // restart on failure. This future never resolves.
    std::future::pending::<()>().await;

    Ok(())
}
