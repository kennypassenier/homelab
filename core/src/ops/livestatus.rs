//! feat-platform-2 · real status per container (homelab-admin, 2026-09-28).
//!
//! `GetState` answered `running: true`, `restarts: 0` and `online: true` for
//! every stack, whatever the machine said (INVENTORY §1). The host now reads
//! the truth on a timer and `GetState` returns the newest reading with the
//! moment it was taken:
//!
//! * one `pvesh get /cluster/resources --type vm` for every guest's status,
//!   cpu, memory and uptime (one call, not one per guest);
//! * one `pct exec <vmid> -- sh -c <PROBE>` per running managed container,
//!   listing its docker containers as `working_dir|service|running|restarts`.
//!
//! Parsing is pure and tested here; the host only runs the commands.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::executor::{Cmd, Executor};

/// Inside a container: one line per docker container. `working_dir` is the
/// compose project directory (`/opt/<stack>/<app>`), which names the app.
pub const PROBE: &str = "command -v docker >/dev/null 2>&1 || exit 0; \
     for c in $(docker ps -aq); do \
     docker inspect --format '{{index .Config.Labels \"com.docker.compose.project.working_dir\"}}|{{index .Config.Labels \"com.docker.compose.service\"}}|{{.State.Running}}|{{.RestartCount}}' \"$c\"; \
     done";

/// One guest as Proxmox reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GuestStatus {
    pub vmid: u16,
    pub running: bool,
    /// Share of one core ×1000 (Proxmox reports a fraction of the guest's
    /// cores; 0.041 → 41). Integer on the wire, like the host's load.
    pub cpu_permille: u32,
    pub mem_used_mb: u32,
    pub mem_max_mb: u32,
    pub uptime_s: u64,
}

/// One app (a compose project under `/opt/<stack>/<app>`) inside a guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppStatus {
    /// Every container of the app is running.
    pub running: bool,
    /// Containers of the app, running or not.
    pub containers: u32,
    /// Sum of the containers' restart counters.
    pub restarts: u32,
}

/// A whole reading: when, every guest, and the apps of each probed guest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LiveStatus {
    pub measured_at: u64,
    pub guests: BTreeMap<u16, GuestStatus>,
    /// vmid → app name → status. A guest missing here was not probed
    /// (stopped, or the probe failed); see `probe_errors`.
    pub apps: BTreeMap<u16, BTreeMap<String, AppStatus>>,
    pub probe_errors: BTreeMap<u16, String>,
}

#[derive(Deserialize)]
struct PveResource {
    vmid: Option<u64>,
    status: Option<String>,
    cpu: Option<f64>,
    mem: Option<u64>,
    maxmem: Option<u64>,
    uptime: Option<u64>,
}

/// `pvesh get /cluster/resources --type vm --output-format json`.
pub fn parse_guests(json: &str) -> Result<BTreeMap<u16, GuestStatus>, String> {
    let rows: Vec<PveResource> = serde_json::from_str(json)
        .map_err(|e| format!("pvesh output is not the expected JSON: {}", e))?;
    let mut out = BTreeMap::new();
    for r in rows {
        let Some(vmid) = r.vmid.and_then(|v| u16::try_from(v).ok()) else {
            continue;
        };
        let mb = |b: Option<u64>| u32::try_from(b.unwrap_or(0) / (1024 * 1024)).unwrap_or(u32::MAX);
        out.insert(
            vmid,
            GuestStatus {
                vmid,
                running: r.status.as_deref() == Some("running"),
                cpu_permille: (r.cpu.unwrap_or(0.0).max(0.0) * 1000.0).round() as u32,
                mem_used_mb: mb(r.mem),
                mem_max_mb: mb(r.maxmem),
                uptime_s: r.uptime.unwrap_or(0),
            },
        );
    }
    Ok(out)
}

/// The probe's lines for one guest of stack `stack`. A container outside
/// `/opt/<stack>/` (started by hand) is ignored: it is no app of the stack.
pub fn parse_apps(stack: &str, probe_out: &str) -> BTreeMap<String, AppStatus> {
    let prefix = format!("/opt/{}/", stack);
    let mut out: BTreeMap<String, AppStatus> = BTreeMap::new();
    for line in probe_out.lines() {
        let parts: Vec<&str> = line.trim().split('|').collect();
        if parts.len() != 4 {
            continue;
        }
        let Some(app) = parts[0].strip_prefix(&prefix) else {
            continue;
        };
        let app = app.trim_end_matches('/');
        if app.is_empty() || app.contains('/') {
            continue;
        }
        let running = parts[2] == "true";
        let restarts: u32 = parts[3].parse().unwrap_or(0);
        let e = out.entry(app.to_string()).or_insert(AppStatus {
            running: true,
            containers: 0,
            restarts: 0,
        });
        e.containers += 1;
        e.running &= running;
        e.restarts = e.restarts.saturating_add(restarts);
    }
    out
}

/// A managed stack as the reader needs it.
#[derive(Debug, Clone)]
pub struct Target {
    pub vmid: u16,
    pub stack: String,
}

/// Take one reading. Never fails as a whole: a guest whose probe fails is
/// named in `probe_errors`, and a failing `pvesh` leaves `guests` empty with
/// the reason under vmid 0.
pub async fn read(exec: &dyn Executor, targets: &[Target], now: u64) -> LiveStatus {
    let mut status = LiveStatus {
        measured_at: now,
        ..Default::default()
    };
    let pvesh = exec
        .run(&Cmd::new(
            "pvesh",
            &[
                "get",
                "/cluster/resources",
                "--type",
                "vm",
                "--output-format",
                "json",
            ],
            30,
        ))
        .await;
    match pvesh {
        Ok(o) if o.code == 0 => match parse_guests(&o.stdout) {
            Ok(g) => status.guests = g,
            Err(e) => {
                status.probe_errors.insert(0, e);
            }
        },
        Ok(o) => {
            status
                .probe_errors
                .insert(0, format!("pvesh exited {}: {}", o.code, o.stderr.trim()));
        }
        Err(e) => {
            status.probe_errors.insert(0, format!("pvesh: {}", e));
        }
    }
    for t in targets {
        let running = status
            .guests
            .get(&t.vmid)
            .map(|g| g.running)
            .unwrap_or(false);
        if !running {
            continue;
        }
        let vmid = t.vmid.to_string();
        let out = exec
            .run(&Cmd::new(
                "pct",
                &["exec", &vmid, "--", "sh", "-c", PROBE],
                30,
            ))
            .await;
        match out {
            Ok(o) if o.code == 0 => {
                status.apps.insert(t.vmid, parse_apps(&t.stack, &o.stdout));
            }
            Ok(o) => {
                status
                    .probe_errors
                    .insert(t.vmid, format!("probe exited {}", o.code));
            }
            Err(e) => {
                status.probe_errors.insert(t.vmid, e.to_string());
            }
        }
    }
    status
}
