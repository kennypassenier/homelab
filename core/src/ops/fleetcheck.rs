//! Y4: hold the repository against reality and report every difference.
//!
//! Everything found by hand on 2026-08-30 had one thing in common: none of it
//! was a failure. kyu's stack record still described a container that had been
//! renamed weeks earlier, so its nightly run failed the hostname guard and the
//! stack quietly auto-disabled. A settings key in `host.toml` replaced the
//! compiled no-touch list rather than adding to it, so a code change had no
//! effect. Three stack files claimed vmids that live containers were using. A
//! Traefik route pointed at an empty container and another at a workstation.
//! Uptime Kuma watched exactly one target. Every one of those looked healthy,
//! and the only reason any of them surfaced is that somebody spent a day
//! looking.
//!
//! This module makes the looking a function. The comparison is pure so it can
//! be exercised without a fleet; gathering the facts is the shell's job.

use serde::{Deserialize, Serialize};

use crate::state::HostState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Something is not doing its job right now.
    Broken,
    /// Working, but drifting — it will bite on the next deploy or outage.
    Drift,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    /// Which stack, container or file this is about.
    pub subject: String,
    /// What is wrong, in one sentence.
    pub what: String,
    /// What to do about it. Every finding carries one (standing rule 11).
    pub remedy: String,
}

/// What the shell reads off the machine so the comparison can stay pure.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    /// vmid → hostname, for every container that exists on the hypervisor.
    pub containers: Vec<(u16, String)>,
    /// Route file name → the address it forwards to, and whether anything
    /// answered there.
    pub routes: Vec<RouteFact>,
    /// Stack directories found in the repository, with the vmid they claim.
    pub stack_files: Vec<(String, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFact {
    pub file: String,
    pub target: String,
    pub answered: bool,
}

/// How stale a backup may be before it counts as a finding. A nightly run
/// that missed one night is noise; two is a pattern (standing rule 27 — a
/// number about patience belongs in configuration, and this is its default).
pub const DEFAULT_BACKUP_MAX_AGE_S: u64 = 48 * 3600;

/// The whole comparison, as one pure function.
pub fn evaluate(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
) -> Vec<Finding> {
    let mut out = Vec::new();

    for (name, st) in &state.stacks {
        match live.containers.iter().find(|(v, _)| *v == st.vmid) {
            None => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("recorded on vmid {}, which does not exist", st.vmid),
                remedy: "the container was removed outside the orchestrator — redeploy it, or remove the stack from host state".into(),
            }),
            Some((_, hostname)) if hostname != &st.hostname => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "recorded as '{}' but vmid {} is really '{}' — every operation on this stack fails the hostname guard",
                    st.hostname, st.vmid, hostname
                ),
                remedy: "re-adopt or redeploy the stack so its record matches the container; this is how kyu's backup stopped for eight weeks".into(),
            }),
            Some(_) => {}
        }

        if !st.enabled {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "disabled — the nightly run skips it entirely".into(),
                remedy: format!(
                    "a failed nightly run auto-disables a stack (H8); fix the cause, then `homelab enable {}`",
                    name
                ),
            });
        }

        let age = now_unix.saturating_sub(st.last_backup);
        if st.last_backup == 0 {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "has never been backed up".into(),
                remedy: "run a backup now and check why the nightly one never did".into(),
            });
        } else if age > backup_max_age_s {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("last backup was {} hours ago", age / 3600),
                remedy: "check the nightly run and the backup target".into(),
            });
        }
    }

    // A stack file that claims a vmid belonging to something else is one
    // hostname guard away from a deploy landing on a live container.
    for (dir, vmid) in &live.stack_files {
        let owned_by_state = state.stacks.values().any(|s| s.vmid == *vmid);
        let exists = live.containers.iter().any(|(v, _)| v == vmid);
        if exists && !owned_by_state {
            out.push(Finding {
                severity: Severity::Drift,
                subject: dir.clone(),
                what: format!(
                    "claims vmid {}, which is a live container this orchestrator does not manage",
                    vmid
                ),
                remedy: "only the hostname guard stands between this file and a deploy onto that container — delete the file or point it at a free vmid".into(),
            });
        }
    }

    for r in &live.routes {
        if !r.answered {
            out.push(Finding {
                severity: Severity::Broken,
                subject: r.file.clone(),
                what: format!("routes to {}, where nothing answers", r.target),
                remedy: "fix the address or delete the route; a dead route is only found when someone needs it".into(),
            });
        }
    }

    out
}

/// The address a route forwards to, as `host/port` for a `/dev/tcp` probe.
///
/// Extracted from the shell so it can be tested: the first live run reported
/// every route in the house dead, and one of the two reasons was here —
/// `https://10.10.5.1` carries no port, so probing it as written asks for a
/// path rather than a socket. The other reason was the shell itself (dash has
/// no /dev/tcp), which no amount of testing this function would have caught.
/// Both were invisible until the check ran against the real fleet once.
pub fn probe_hostport(target: &str) -> String {
    let (scheme, rest) = match target.split_once("://") {
        Some((s, r)) => (s, r),
        None => ("", target),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.rsplit_once(':') {
        // An IPv6 literal has colons of its own; only a numeric tail is a port.
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            format!("{}/{}", host, port)
        }
        _ => format!("{}/{}", authority, if scheme == "https" { 443 } else { 80 }),
    }
}
