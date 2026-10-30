//! The scheduler: the status loop, and the loop that wakes every 20 minutes
//! and, inside the night window, runs the night (backups, updates, the host's
//! own jobs, the reads and the fleet check). One file per part of the night.

mod fleet_check;
mod gate;
mod host_jobs;
mod night;
mod reads;
mod stacks;
mod status;

use night::Night;
pub(crate) use status::status_loop;

use crate::*;

pub(crate) async fn scheduler_loop(state: AppState) {
    let exec = RealExecutor;
    // fix-129: the first look soon after start, then every 20 minutes.
    let mut wait = Duration::from_secs(SCHEDULER_FIRST_CHECK_S);
    loop {
        tokio::time::sleep(wait).await;
        wait = Duration::from_secs(20 * 60);
        // fix-52 residual: the loop woke up on its own, which is the one
        // thing an `await` stuck forever cannot do.
        touch_scheduler_heartbeat(&state);
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
        let Some(gate) = gate::open(&state, &exec).await else {
            continue;
        };
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let snapshot = match store.load().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("scheduler: state unreadable — skipping this tick: {}", e);
                continue;
            }
        };
        let night = Night::plan(&state, &exec, gate, &snapshot);
        run_night(&night, snapshot).await;
    }
}

/// The night in its fixed order: backups, then each stack's updates, then the
/// host's own jobs, then what is read and kept tidy.
async fn run_night(night: &Night<'_>, snapshot: homelab_core::state::HostState) {
    let backup_done = stacks::backup_batch(night, &snapshot.stacks).await;
    let updates_parked = snapshot.updates_parked.clone();
    for (name, st) in snapshot.stacks {
        stacks::stack_night(night, name, st, &backup_done, &updates_parked).await;
    }
    host_jobs::host_meta(night).await;
    host_jobs::restore_drill(night).await;
    host_jobs::device_config(night).await;
    host_jobs::home_address(night).await;
    host_jobs::second_copy(night).await;
    host_jobs::integrity_check(night).await;
    host_jobs::zfs(night).await;
    reads::pins(night).await;
    reads::edge(night).await;
    // Managed tools (latch): keep them current under their follow rule.
    tools_round(night.state, night.exec).await;
    fleet_check::fleet_check(night).await;
    reads::housekeeping(night).await;
}
