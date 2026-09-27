//! The systemd units and rollback script that run the host daemon on pve.
//!
//! Until 2026-09-27 these existed only on pve: the DR runbook recreated a
//! minimal unit without `Type=notify`, `WatchdogSec` or `OnFailure=`, and a
//! host rebuilt from it would have run without watchdog and without the
//! self-update rollback, silently (expert panel, daemon-units-outside-repo).
//! They now ship inside the binary: every self-update puts them in place
//! before the restart, and doctor names any that differ on pve.

/// One file the daemon's supervision depends on.
#[derive(Debug, Clone, Copy)]
pub struct HostUnit {
    pub path: &'static str,
    pub content: &'static str,
    pub mode: u32,
}

/// pve's journal cap (Kenny, 2026-09-27). journald reads it only when it
/// restarts, so the self-update restarts journald when this file changed.
pub const JOURNALD_CAP: &str = "/etc/systemd/journald.conf.d/homelab-limits.conf";

pub const UNITS: &[HostUnit] = &[
    HostUnit {
        path: "/etc/systemd/system/homelab-host.service",
        content: include_str!("../assets/host-units/homelab-host.service"),
        mode: 0o644,
    },
    HostUnit {
        path: "/etc/systemd/system/homelab-host-rollback.service",
        content: include_str!("../assets/host-units/homelab-host-rollback.service"),
        mode: 0o644,
    },
    HostUnit {
        path: JOURNALD_CAP,
        content: include_str!("../assets/host-units/homelab-journald.conf"),
        mode: 0o644,
    },
    HostUnit {
        path: "/usr/local/lib/homelab-rollback.sh",
        content: include_str!("../assets/host-units/homelab-rollback.sh"),
        mode: 0o755,
    },
];

/// The paths whose live content differs from what the binary carries.
/// `live` is each unit's path with what pve holds (`None`: absent).
pub fn drift(live: &[(&str, Option<String>)]) -> Vec<String> {
    live.iter()
        .filter(|(path, got)| {
            let want = UNITS.iter().find(|u| u.path == *path).map(|u| u.content);
            got.as_deref() != want
        })
        .map(|(path, _)| path.to_string())
        .collect()
}
