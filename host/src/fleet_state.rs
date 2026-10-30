//! The fleet state the dashboard reads: live app and health lookups, the sealed-env refresh and the self-release verdict.

use crate::*;

/// feat-platform-2: one app's (running, restarts) from a reading. No
/// reading yet, or its guest was not probed: the old answer (running, 0),
/// said as such by `FleetState.status_measured_at` being None. Probed and
/// absent: the app has no container at all, which is not running.
pub(crate) fn live_app(
    reading: Option<&homelab_core::ops::livestatus::LiveStatus>,
    vmid: u16,
    app: &str,
) -> (bool, u32) {
    let Some(apps) = reading.and_then(|r| r.apps.get(&vmid)) else {
        return (true, 0);
    };
    match apps.get(app) {
        Some(a) => (a.running, a.restarts),
        None => (false, 0),
    }
}

/// redesign-stackhub-2: an app's own health check from the newest reading
/// (docker's healthcheck, or `systemctl is-active` for a native unit); None
/// when the app declares none or was not read.
pub(crate) fn live_health(
    reading: Option<&homelab_core::ops::livestatus::LiveStatus>,
    vmid: u16,
    app: &str,
) -> Option<String> {
    reading?.apps.get(&vmid)?.get(app)?.health.clone()
}

/// redesign-stackhub-1: how often the status loop reads every stack's
/// "Env sealed" verdict. It asks each container per secret file, so it runs
/// at a slower cadence than the guest status; a seal-env action reads it
/// again at once.
pub(crate) const ENV_SEALED_INTERVAL_S: u64 = 600;

/// redesign-stackhub-1: read every stack's "Env sealed" verdict now and
/// keep it for the fleet state.
pub(crate) async fn refresh_env_sealed(state: &AppState, exec: &RealExecutor) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let hs = store.load().await.unwrap_or_default();
    let verdicts =
        homelab_core::ops::facts::sealed_verdicts(exec, &state.config.state_dir, &hs).await;
    if let Ok(mut slot) = state.env_sealed.write() {
        *slot = verdicts;
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
    // H16: real capacity numbers (C6) — free -m, nproc, loadavg,
    // and committed RAM summed from the stored manifests.
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
    let cap = capacity_numbers(&free_out, &nproc_out, &loadavg, &hs);
    let df = exec
        .run(&Cmd::new(
            "df",
            &["--output=pcent", &state.config.state_dir],
            20,
        ))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
        })
        .unwrap_or(0);
    // fix-222: cached at DISK_DETAIL_REFRESH_S, never re-gathered on every
    // status poll — a fresh read here would run `du` over the whole root
    // filesystem on a schedule governed by `status_interval_s`, which is
    // seconds, not the 15 minutes this is worth re-measuring at.
    const DISK_DETAIL_REFRESH_S: u64 = 900;
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
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
    let disk_detail = state.disk_detail.read().ok().and_then(|d| d.clone());
    // redesign-host-4: the host's uptime, and whether this binary is a
    // signed release (verified once, against the signature recorded when it
    // was installed).
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
    // feat-platform-2: the newest status reading, when there is one.
    let reading = state.live_status.read().ok().and_then(|r| r.clone());
    // Keyed by hostname here: the map is keyed by the state's stack name,
    // which `values()` below does not carry.
    let sealed: std::collections::BTreeMap<String, bool> = {
        let by_name = state
            .env_sealed
            .read()
            .map(|m| m.clone())
            .unwrap_or_default();
        hs.stacks
            .iter()
            .filter_map(|(name, st)| by_name.get(name).map(|v| (st.hostname.clone(), *v)))
            .collect()
    };
    let stacks = hs
        .stacks
        .values()
        .map(|s| homelab_proto::StackView {
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
            // fix-160: a native stack's units are its apps, also on
            // stacks adopted before install-native recorded them.
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
                    let (running, restarts) = live_app(reading.as_ref(), s.vmid, a);
                    homelab_proto::AppView {
                        name: a.clone(),
                        running,
                        restarts,
                        health: live_health(reading.as_ref(), s.vmid, a),
                    }
                })
                .collect(),
            drift: false, // computed client-side from applied_hash
            applied_hash: s.applied_hash.clone(),
            // redesign-stackhub-1: what the status loop read, not a
            // hard-coded true; unread goes out as unknown (None) and the
            // old field keeps its old meaning for older readers.
            env_sealed: sealed.get(&s.hostname).copied().unwrap_or(true),
            env_sealed_read: sealed.get(&s.hostname).copied(),
            online: reading
                .as_ref()
                .and_then(|r| r.guests.get(&s.vmid))
                .map(|g| g.running)
                .unwrap_or(true),
            enabled: s.enabled,
            usage: reading
                .as_ref()
                .and_then(|r| r.guests.get(&s.vmid))
                .map(|g| homelab_proto::GuestUsage {
                    cpu_permille: g.cpu_permille,
                    ram_used_mb: g.mem_used_mb,
                    ram_max_mb: g.mem_max_mb,
                    uptime_s: g.uptime_s,
                }),
        })
        .collect();
    homelab_proto::FleetState {
        status_measured_at: reading.as_ref().map(|r| r.measured_at),
        host: homelab_proto::HostView {
            home_address: hs.home_address.clone(),
            trusted_sources: state.config.trusted_sources.clone(),
            name: "pve-01".into(),
            // fix-175: was a fixed 0; now the `status_loop` /proc/stat
            // delta (None before the first poll has a predecessor).
            cpu_pct: state.host_cpu_pct.read().ok().and_then(|p| *p),
            // feat-platform-2: was a fixed 0; used over total, both
            // from `free -m` (C6).
            ram_pct: if cap.0 > 0 {
                u64::from(cap.1) * 100 / u64::from(cap.0)
            } else {
                0
            },
            disk_pct: df,
            tls_fingerprint: fingerprint,
            ram_total_mb: cap.0,
            ram_used_mb: cap.1,
            ram_committed_mb: cap.2,
            cores_total: cap.3,
            load1_x100: cap.4,
            disk_detail,
            uptime_s,
            release: Some(release),
            // redesign-host-4: every guest Proxmox reports, managed or not
            // (the Host page's Containers table).
            guests_usage: reading.as_ref().map(|r| {
                r.guests
                    .values()
                    .map(|g| homelab_proto::GuestUse {
                        vmid: g.vmid,
                        usage: homelab_proto::GuestUsage {
                            cpu_permille: g.cpu_permille,
                            ram_used_mb: g.mem_used_mb,
                            ram_max_mb: g.mem_max_mb,
                            uptime_s: g.uptime_s,
                        },
                    })
                    .collect()
            }),
        },
        stacks,
    }
}

/// redesign-host-4: the running binary (`/proc/self/exe`, which still reads
/// after the file on disk was replaced) against the signature recorded when
/// it was installed.
pub(crate) fn own_release_verdict(state_dir: &str) -> homelab_proto::ReleaseVerdict {
    match std::fs::read("/proc/self/exe") {
        Ok(exe) => {
            let v = homelab_core::release_sig::running_verdict(
                std::path::Path::new(state_dir),
                SELF_ASSET,
                &exe,
            );
            homelab_proto::ReleaseVerdict {
                signed: v.signed,
                detail: v.detail,
            }
        }
        Err(e) => homelab_proto::ReleaseVerdict {
            signed: false,
            detail: format!("the host could not read its own binary to check it: {e}"),
        },
    }
}

/// The host binary's asset name in a homelab release.
pub(crate) const SELF_ASSET: &str = "homelab-host";
