//! The fleet as the browser shows it (feat-overview-1, skeleton cut).
//!
//! The host answers `GetState` with a `FleetState`; the browser gets this
//! smaller, already-sorted view plus the moment it was measured, so the page
//! can say "measured 12 s ago" (feat-overview-4) without trusting its own
//! clock for the measurement itself.

use homelab_proto::FleetState;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FleetView {
    /// Unix seconds at which the admin received this state from the host.
    pub measured_at: u64,
    pub host: HostSummary,
    pub stacks: Vec<StackSummary>,
    pub counts: Counts,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostSummary {
    pub name: String,
    pub cpu_pct: u64,
    pub ram_used_mb: u32,
    pub ram_total_mb: u32,
    pub disk_pct: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StackSummary {
    pub name: String,
    pub vmid: u16,
    pub online: bool,
    pub enabled: bool,
    pub apps_running: usize,
    pub apps_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub stacks: usize,
    pub online: usize,
    pub parked: usize,
}

/// Sort by vmid so the page order matches `pct list` and never jumps
/// between two refreshes.
pub fn fleet_view(state: &FleetState, measured_at: u64) -> FleetView {
    let mut stacks: Vec<StackSummary> = state
        .stacks
        .iter()
        .map(|s| StackSummary {
            name: s.name.clone(),
            vmid: s.vmid,
            online: s.online,
            enabled: s.enabled,
            apps_running: s.apps.iter().filter(|a| a.running).count(),
            apps_total: s.apps.len(),
        })
        .collect();
    stacks.sort_by(|a, b| a.vmid.cmp(&b.vmid).then_with(|| a.name.cmp(&b.name)));
    let counts = Counts {
        stacks: stacks.len(),
        online: stacks.iter().filter(|s| s.online).count(),
        parked: stacks.iter().filter(|s| !s.enabled).count(),
    };
    FleetView {
        measured_at,
        host: HostSummary {
            name: state.host.name.clone(),
            cpu_pct: state.host.cpu_pct,
            ram_used_mb: state.host.ram_used_mb,
            ram_total_mb: state.host.ram_total_mb,
            disk_pct: state.host.disk_pct,
        },
        stacks,
        counts,
    }
}
