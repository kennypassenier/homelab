//! Each stack's record in host state against the containers that exist and
//! the stack files in the repository.

use super::backup_age::{evaluate_backup_age, unkept_notes};
use super::{Finding, LiveFacts, Severity};
use crate::state::{HostState, StackState};

/// Every stack-level check against the records, in report order.
pub(super) fn stack_findings(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(missing_template_findings(state, live));
    out.extend(per_stack_findings(state, live, now_unix, backup_max_age_s));
    out.extend(stack_file_claims(state, live));
    out.extend(deleted_stack_findings(state, live));
    out
}

/// A stack that clones a template which is not there rebuilds into
/// nothing — and it fails at the moment you need the rebuild most. The
/// shape this guards against was found on 2026-09-01: the scaffold default
/// still named `clone:999`, the v1 golden image, two generations after
/// every live stack had moved to 997/998. Nobody noticed because nobody
/// scaffolds a stack often, and the eleven that exist carry their template
/// by hand.
fn missing_template_findings(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(rest) = m.lxc.template.trim_matches('"').strip_prefix("clone:") else {
            continue;
        };
        let Ok(tmpl_vmid) = rest.trim().parse::<u16>() else {
            continue;
        };
        if !live.containers.iter().any(|(v, _)| *v == tmpl_vmid) {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "clones template {}, which does not exist on the hypervisor",
                    tmpl_vmid
                ),
                remedy: format!(
                    "point {}'s lxc.template at a golden template that is there, \
                     or rebuild {} with `homelab template-build`",
                    name, tmpl_vmid
                ),
            });
        }
    }
    out
}

/// The record, the enabled flag, the parked updates, the deliberate gaps and
/// the backup age of every stack, in that order.
fn per_stack_findings(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        out.extend(record_finding(name, st, live));
        out.extend(disabled_finding(name, st));
        out.extend(parked_finding(state, name));
        out.extend(unkept_notes(name, st));
        out.extend(evaluate_backup_age(
            name,
            st,
            live,
            now_unix,
            backup_max_age_s,
        ));
    }
    out
}

/// The vmid the record names, and the hostname the guard compares.
fn record_finding(name: &str, st: &StackState, live: &LiveFacts) -> Option<Finding> {
    match live.containers.iter().find(|(v, _)| *v == st.vmid) {
        None => Some(Finding {
            severity: Severity::Broken,
            subject: name.to_string(),
            what: format!("recorded on vmid {}, which does not exist", st.vmid),
            remedy: "the container was removed outside the orchestrator — redeploy it, or remove the stack from host state".into(),
        }),
        Some((_, hostname)) if hostname != &st.hostname => Some(Finding {
            severity: Severity::Broken,
            subject: name.to_string(),
            what: format!(
                "recorded as '{}' but vmid {} is really '{}' — every operation on this stack fails the hostname guard",
                st.hostname, st.vmid, hostname
            ),
            remedy: "re-adopt or redeploy the stack so its record matches the container; until then its backups and updates stop".into(),
        }),
        Some(_) => None,
    }
}

fn disabled_finding(name: &str, st: &StackState) -> Option<Finding> {
    (!st.enabled).then(|| Finding {
        severity: Severity::Broken,
        subject: name.to_string(),
        what: "disabled — the nightly run skips it entirely".into(),
        remedy: format!(
            "a failed nightly run auto-disables a stack; fix the cause, then `homelab enable {}`",
            name
        ),
    })
}

/// fix-59: a failed nightly update parks the updates only. The backup
/// still runs, so nothing else would ever say the updates stopped.
fn parked_finding(state: &HostState, name: &str) -> Option<Finding> {
    let since = state.updates_parked.get(name)?;
    Some(Finding {
        severity: Severity::Broken,
        subject: name.to_string(),
        what: format!(
            "automatic updates parked since {} after a failed nightly update or a \
             `homelab rollback-native`; the nightly backup still runs",
            crate::state::ymd(*since)
        ),
        remedy: format!(
            "read that night's update transcript, fix or pin the image or release, \
             then `homelab enable {}` resumes the updates",
            name
        ),
    })
}

/// A stack file that claims a vmid belonging to something else is one
/// hostname guard away from a deploy landing on a live container.
fn stack_file_claims(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
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
    out
}

/// step-22 / ask-8: the other direction. A stack in host state whose
/// directory is gone from the repository is still running, still backed
/// up and still registered everywhere — and nothing compared the two, so
/// deleting a stack directory changed nothing and said nothing. Kenny's
/// answer (form 2026-09-27, `Via homelab apply`): it is reported here
/// until `homelab apply` destroys it after its name is typed.
///
/// Only when the client sent its stack files at all: an empty list means
/// the check ran from somewhere without the repository, and judging
/// against nothing would call every stack deleted.
fn deleted_stack_findings(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    if live.stack_files.is_empty() {
        return Vec::new();
    }
    let dirs: std::collections::BTreeSet<&str> = live
        .stack_files
        .iter()
        .map(|(d, _)| {
            let d = d.trim_end_matches('/');
            d.rsplit('/').next().unwrap_or(d)
        })
        .collect();
    state
        .stacks
        .iter()
        .filter(|(name, _)| !dirs.contains(name.as_str()))
        .map(|(name, st)| Finding {
            severity: Severity::Drift,
            subject: name.clone(),
            what: format!(
                "is in host state (vmid {}) but has no stack file in the repository — \
                 stacks/{}/ is gone",
                st.vmid, name
            ),
            remedy: format!(
                "`homelab apply` lists it and destroys it after you type its name; put \
                 stacks/{}/ back to keep it",
                name
            ),
        })
        .collect()
}
