//! Whether this tick is one the night may run in: the standing checks, the
//! scheduler switch, the host-local hour and the night window.

use crate::*;

/// What a tick that may run the night knows about the moment.
pub(super) struct Gate {
    pub(super) hour: u8,
    pub(super) local_hour: u8,
    pub(super) now: u64,
    pub(super) tiers: Vec<homelab_proto::RetentionTier>,
}

/// `Some` when the night may run now, `None` when this tick has nothing to do.
pub(super) async fn open(state: &AppState, exec: &RealExecutor) -> Option<Gate> {
    {
        let tick_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
        record_state(&store, "ensure standing checks", |s| {
            homelab_core::ops::manualchecks::ensure_standing_checks(s, tick_now)
        })
        .await;
    }
    let (hour, tiers) = {
        let s = state
            .settings
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        match s.backup_hour {
            Some(h) => (h, s.retention.clone()),
            None => return None, // scheduler disabled
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Host-local hour without pulling in chrono: read from `date`.
    let local_hour = match exec.run(&Cmd::new("date", &["+%H"], 10)).await {
        Ok(out) => parse_local_hour(&out.stdout),
        Err(_) => None,
    };
    let Some(local_hour) = local_hour else {
        tracing::error!(
            "scheduler: cannot determine local hour ('date' failed) — nightly run skipped THIS TICK; investigate"
        );
        return None;
    };
    if !in_night_window(hour, local_hour) {
        return None;
    }
    Some(Gate {
        hour,
        local_hour,
        now,
        tiers,
    })
}
