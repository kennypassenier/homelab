//! The status loop: one reading of every container every `status_interval_s`.

use crate::*;

/// feat-platform-2: one reading every `status_interval_s`, the managed
/// stacks taken from state.json each time so a new stack is read at once.
pub(crate) async fn status_loop(state: AppState) {
    let exec = RealExecutor;
    let mut sealed_at: Option<u64> = None;
    loop {
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let hs = store.load().await.unwrap_or_default();
        let targets: Vec<homelab_core::ops::livestatus::Target> = hs
            .stacks
            .iter()
            .map(|(name, s)| homelab_core::ops::livestatus::Target {
                vmid: s.vmid,
                stack: name.clone(),
                units: s.natives.iter().map(|n| n.unit.clone()).collect(),
            })
            .collect();
        let reading = homelab_core::ops::livestatus::read(&exec, &targets, unix_now()).await;
        for (vmid, why) in &reading.probe_errors {
            tracing::debug!(vmid, "status reading :: {}", why);
        }
        if let Ok(mut slot) = state.live_status.write() {
            *slot = Some(reading);
        }
        // redesign-stackhub-1: after the status reading, so the first one is
        // not held up by asking every container about its secret files.
        if sealed_at.is_none_or(|t| unix_now().saturating_sub(t) >= ENV_SEALED_INTERVAL_S) {
            refresh_env_sealed(&state, &exec).await;
            sealed_at = Some(unix_now());
        }
        // fix-175: the host's own CPU, same cadence as the guest reading —
        // busy share since the previous poll, from two /proc/stat samples.
        // The first poll of this daemon's life has no previous sample, so
        // it stores one and reports unknown rather than a fabricated 0.
        if let Ok(stat) = exec.read_file("/proc/stat").await
            && let Some(curr) = homelab_core::ops::hostcpu::parse_proc_stat(&stat)
        {
            let prev = state.host_cpu_prev.read().ok().and_then(|p| *p);
            let pct = prev.and_then(|p| homelab_core::ops::hostcpu::cpu_pct(&p, &curr));
            if let Ok(mut slot) = state.host_cpu_pct.write() {
                *slot = pct.map(u64::from);
            }
            if let Ok(mut slot) = state.host_cpu_prev.write() {
                *slot = Some(curr);
            }
        }
        tokio::time::sleep(Duration::from_secs(state.config.status_interval_s)).await;
    }
}
