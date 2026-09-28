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
    /// Unix seconds of the host's status reading when it has one
    /// (feat-platform-2), otherwise when the admin received the state.
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
    /// feat-overview-2: the host page's extra facts, as the host reports
    /// them in `HostView`.
    pub ram_committed_mb: u32,
    pub cores_total: u16,
    /// 1-minute load average ×100 (250 = 2.50).
    pub load1_x100: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StackSummary {
    pub name: String,
    pub vmid: u16,
    pub online: bool,
    pub enabled: bool,
    pub apps_running: usize,
    pub apps_total: usize,
    /// Sum of the apps' container restarts (feat-platform-2).
    pub restarts: u32,
    pub ram_used_mb: Option<u32>,
    pub ram_max_mb: Option<u32>,
    /// feat-stacks-1: the stack page's detail, from the same snapshot.
    pub hostname: String,
    pub apps: Vec<AppSummary>,
    /// Seconds since the guest started, at the host's last status reading.
    pub uptime_s: Option<u64>,
    /// arch-deploy-guard: where the last deploy came from, as the host
    /// recorded it ("a1b2c3d4e5f6 + 1 uncommitted file(s)").
    pub applied_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSummary {
    pub name: String,
    pub running: bool,
    pub restarts: u32,
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
            restarts: s.apps.iter().map(|a| a.restarts).sum(),
            ram_used_mb: s.usage.as_ref().map(|u| u.ram_used_mb),
            ram_max_mb: s.usage.as_ref().map(|u| u.ram_max_mb),
            hostname: s.hostname.clone(),
            apps: s
                .apps
                .iter()
                .map(|a| AppSummary {
                    name: a.name.clone(),
                    running: a.running,
                    restarts: a.restarts,
                })
                .collect(),
            uptime_s: s.usage.as_ref().map(|u| u.uptime_s),
            applied_source: s.applied_source.clone(),
        })
        .collect();
    stacks.sort_by(|a, b| a.vmid.cmp(&b.vmid).then_with(|| a.name.cmp(&b.name)));
    let counts = Counts {
        stacks: stacks.len(),
        online: stacks.iter().filter(|s| s.online).count(),
        parked: stacks.iter().filter(|s| !s.enabled).count(),
    };
    FleetView {
        measured_at: state.status_measured_at.unwrap_or(measured_at),
        host: HostSummary {
            name: state.host.name.clone(),
            cpu_pct: state.host.cpu_pct,
            ram_used_mb: state.host.ram_used_mb,
            ram_total_mb: state.host.ram_total_mb,
            disk_pct: state.host.disk_pct,
            ram_committed_mb: state.host.ram_committed_mb,
            cores_total: state.host.cores_total,
            load1_x100: state.host.load1_x100,
        },
        stacks,
        counts,
    }
}
