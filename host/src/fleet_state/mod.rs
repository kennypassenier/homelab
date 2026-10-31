//! The fleet state the dashboard reads: the host view, each stack's view and what is cached between polls.

use crate::*;

mod live;
mod release;

pub(crate) use live::*;
pub(crate) use release::*;

type Reading = homelab_core::ops::livestatus::LiveStatus;

/// H16: real capacity numbers (C6) — `free -m`, `nproc`, loadavg, and the RAM
/// committed by the stored manifests.
async fn capacity(
    exec: &RealExecutor,
    hs: &homelab_core::state::HostState,
) -> (u32, u32, u32, u16, u32) {
    let free_out = exec
        .run(&Cmd::new("free", &["-m"], 15))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let nproc_out = exec
        .run(&Cmd::new("nproc", &[], 15))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let loadavg = exec.read_file("/proc/loadavg").await.unwrap_or_default();
    capacity_numbers(&free_out, &nproc_out, &loadavg, hs)
}

/// Used percent of the state directory's filesystem; 0 when `df` fails.
async fn disk_used_pct(exec: &RealExecutor, state_dir: &str) -> u64 {
    exec.run(&Cmd::new("df", &["--output=pcent", state_dir], 20))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
        })
        .unwrap_or(0)
}

/// fix-222: cached at `DISK_DETAIL_REFRESH_S`, never re-gathered on every
/// status poll — a fresh read here would run `du` over the whole root
/// filesystem on a schedule governed by `status_interval_s`, which is seconds,
/// not the 15 minutes this is worth re-measuring at.
async fn cached_disk_detail(
    state: &AppState,
    exec: &RealExecutor,
) -> Option<homelab_proto::HostDiskDetail> {
    const DISK_DETAIL_REFRESH_S: u64 = 900;
    let now_s = unix_now();
    let stale = state
        .disk_detail
        .read()
        .ok()
        .and_then(|d| d.clone())
        .is_none_or(|d| now_s.saturating_sub(d.measured_at) >= DISK_DETAIL_REFRESH_S);
    if stale {
        let fresh = gather_host_disk_detail(exec, now_s).await;
        if let Ok(mut w) = state.disk_detail.write() {
            *w = Some(fresh);
        }
    }
    state.disk_detail.read().ok().and_then(|d| d.clone())
}

/// Which stacks' secret files the vault has copies of, keyed by hostname: the
/// state's map is keyed by stack name, which `StackView` does not carry.
fn sealed_by_hostname(
    state: &AppState,
    hs: &homelab_core::state::HostState,
) -> std::collections::BTreeMap<String, bool> {
    let by_name = state
        .env_sealed
        .read()
        .map(|m| m.clone())
        .unwrap_or_default();
    hs.stacks
        .iter()
        .filter_map(|(name, st)| by_name.get(name).map(|v| (st.hostname.clone(), *v)))
        .collect()
}

fn usage_of(g: &homelab_core::ops::livestatus::GuestStatus) -> homelab_proto::GuestUsage {
    homelab_proto::GuestUsage {
        cpu_permille: g.cpu_permille,
        ram_used_mb: g.mem_used_mb,
        ram_max_mb: g.mem_max_mb,
        uptime_s: g.uptime_s,
    }
}

/// One stack as the dashboard sees it, with what the status loop read.
fn stack_view(
    s: &homelab_core::state::StackState,
    reading: Option<&Reading>,
    sealed: &std::collections::BTreeMap<String, bool>,
) -> homelab_proto::StackView {
    let guest = reading.and_then(|r| r.guests.get(&s.vmid));
    homelab_proto::StackView {
        native: s.is_native(),
        applied_source: s.applied_source.clone(),
        component_digests: s.component_digests.clone(),
        name: s
            .hostname
            .rsplit("-app-")
            .next()
            .unwrap_or(&s.hostname)
            .to_string(),
        vmid: s.vmid,
        hostname: s.hostname.clone(),
        // fix-160: a native stack's units are its apps, also on stacks adopted
        // before install-native recorded them.
        apps: s
            .apps
            .iter()
            .chain(
                s.natives
                    .iter()
                    .map(|n| &n.unit)
                    .filter(|u| !s.apps.contains(u)),
            )
            .map(|a| {
                let (running, restarts) = live_app(reading, s.vmid, a);
                homelab_proto::AppView {
                    name: a.clone(),
                    running,
                    restarts,
                    health: live_health(reading, s.vmid, a),
                }
            })
            .collect(),
        drift: false, // computed client-side from applied_hash
        applied_hash: s.applied_hash.clone(),
        // redesign-stackhub-1: what the status loop read, not a hard-coded
        // true; unread goes out as unknown (None) and the old field keeps its
        // old meaning for older readers.
        env_sealed: sealed.get(&s.hostname).copied().unwrap_or(true),
        env_sealed_read: sealed.get(&s.hostname).copied(),
        online: guest.map(|g| g.running).unwrap_or(true),
        enabled: s.enabled,
        usage: guest.map(usage_of),
    }
}

/// The host's own view: capacity, disk, uptime, release and every guest
/// Proxmox reports (redesign-host-4: the Host page's Containers table).
async fn host_view(
    state: &AppState,
    exec: &RealExecutor,
    hs: &homelab_core::state::HostState,
    reading: Option<&Reading>,
) -> homelab_proto::HostView {
    let cap = capacity(exec, hs).await;
    let disk_pct = disk_used_pct(exec, &state.config.state_dir).await;
    let disk_detail = cached_disk_detail(state, exec).await;
    // redesign-host-4: the host's uptime, and whether this binary is a signed
    // release (verified once, against the signature recorded at install).
    let uptime_s = exec
        .read_file("/proc/uptime")
        .await
        .ok()
        .and_then(|t| homelab_core::ops::fleetcheck::parse_proc_uptime(&t));
    let release = state
        .release_verdict
        .get_or_init(|| own_release_verdict(&state.config.state_dir))
        .clone();
    let (_, fingerprint) = tls::ensure_cert(&state.config.state_dir, "homelab-host").unwrap_or((
        tls::CertPaths {
            cert_pem: String::new(),
            key_pem: String::new(),
        },
        "unknown".into(),
    ));
    homelab_proto::HostView {
        home_address: hs.home_address.clone(),
        trusted_sources: state.config.trusted_sources.clone(),
        name: "pve-01".into(),
        // fix-175: the `status_loop` /proc/stat delta (None before the first
        // poll has a predecessor).
        cpu_pct: state.host_cpu_pct.read().ok().and_then(|p| *p),
        // feat-platform-2: used over total, both from `free -m` (C6).
        ram_pct: if cap.0 > 0 {
            u64::from(cap.1) * 100 / u64::from(cap.0)
        } else {
            0
        },
        disk_pct,
        tls_fingerprint: fingerprint,
        ram_total_mb: cap.0,
        ram_used_mb: cap.1,
        ram_committed_mb: cap.2,
        cores_total: cap.3,
        load1_x100: cap.4,
        disk_detail,
        uptime_s,
        release: Some(release),
        guests_usage: reading.map(|r| {
            r.guests
                .values()
                .map(|g| homelab_proto::GuestUse {
                    vmid: g.vmid,
                    usage: usage_of(g),
                })
                .collect()
        }),
    }
}

/// fix-68: the fleet snapshot `GetState` broadcasts and `Status` now
/// returns directly, built once so neither drifts from the other.
pub(crate) async fn build_fleet_state(
    state: &AppState,
    exec: &RealExecutor,
) -> homelab_proto::FleetState {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let hs = store.load().await.unwrap_or_default();
    // feat-platform-2: the newest status reading, when there is one.
    let reading = state.live_status.read().ok().and_then(|r| r.clone());
    let sealed = sealed_by_hostname(state, &hs);
    let stacks = hs
        .stacks
        .values()
        .map(|s| stack_view(s, reading.as_ref(), &sealed))
        .collect();
    homelab_proto::FleetState {
        status_measured_at: reading.as_ref().map(|r| r.measured_at),
        host: host_view(state, exec, &hs, reading.as_ref()).await,
        stacks,
    }
}
