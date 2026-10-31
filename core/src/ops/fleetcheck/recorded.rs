//! Checks that read only the host's own recorded state: restore checks, the
//! host-meta backup, notifications and half-finished deploys.

use super::{Finding, Severity};
use crate::state::HostState;

/// fix-147 (restore-check-failure, Kenny 2026-09-27, form "Keuzes helpers"):
/// a deploy that found data empty and could not check its backup goes on
/// (backup-target trouble never blocks a deploy, E3), so the service may be
/// running on empty data while a history exists. fix-54 made that a warning
/// in the transcript; this keeps it standing, on its stack, until a later
/// check of the same directory or unit succeeds (a deploy, or a restore).
pub fn evaluate_restore_checks(state: &HostState) -> Vec<Finding> {
    state
        .restore_check_failures
        .values()
        .map(|r| Finding {
            severity: Severity::Broken,
            subject: r.stack.clone(),
            what: format!(
                "the deploy of {} found {} empty and could not check its backup ({}) — it \
                 started without that data, and a history may exist",
                crate::state::ymd(r.at),
                r.what,
                r.why
            ),
            remedy: format!(
                "check the backup target, then restore the data if it held any (`homelab \
                 restore stacks/{} --app <app>`, or op-11 for a native unit); a later deploy \
                 whose check of it succeeds, or a restore, clears this",
                r.stack
            ),
        })
        .collect()
}

/// fix-111: older than this, the host-meta backup has stopped: two nights.
pub const HOST_META_MAX_AGE_S: u64 = 48 * 3600;

/// fix-111 (host-meta-gaps, 2026-09-27): the daemon's own repository — the
/// vault with the restic password, state.json, TLS and host.toml — had no
/// doctor line and no finding, so a host-meta backup that stopped was
/// reported nowhere until the host was lost. A host that manages no stack
/// yet is left alone: there is nothing of its own worth keeping.
pub fn evaluate_host_meta(state: &HostState, now: u64, max_age_s: u64) -> Vec<Finding> {
    if state.stacks.is_empty() {
        return Vec::new();
    }
    let age = now.saturating_sub(state.last_host_meta);
    if state.last_host_meta != 0 && age <= max_age_s {
        return Vec::new();
    }
    vec![Finding {
        severity: Severity::Broken,
        subject: "host-meta".into(),
        what: if state.last_host_meta == 0 {
            "the daemon's own state (vault, state.json, TLS, host.toml) has never been backed up"
                .into()
        } else {
            format!(
                "the daemon's own state was last backed up {} hours ago",
                age / 3600
            )
        },
        remedy: "a lost host disk now loses the vault and the restic password with it; run \
                 `homelab backup-host-meta` and read why the nightly one failed"
            .into(),
    }]
}

/// G16: is the path by which Kenny learns anything still working?
///
/// The circularity is the point and cannot be engineered away: if every
/// notification route is down, this finding cannot reach him by notification
/// either. What it can do is be there in `homelab check` and the TUI, so the
/// question "why has it been so quiet" has an answer other than a guess.
pub fn evaluate_notify(state: &HostState, now: u64) -> Vec<Finding> {
    if state.last_notify_failed <= state.last_notify_ok {
        return Vec::new();
    }
    let ago = now.saturating_sub(state.last_notify_failed) / 60;
    let last_ok = last_ok_words(state);
    vec![Finding {
        severity: Severity::Broken,
        subject: "notifications".into(),
        what: format!(
            "no route accepted the last notification ({} min ago{}){}",
            ago,
            state
                .last_notify_error
                .as_ref()
                .map(|e| format!(": {}", e))
                .unwrap_or_default(),
            format_args!(" — {}", last_ok)
        ),
        remedy: "check the route in `notify_url` and `notify_fallback_webhook` in host.toml — \
                 while this stands, every warning this host produces is going nowhere, \
                 including this one"
            .into(),
    }]
}

/// How the last successful notification compares with the failure.
fn last_ok_words(state: &HostState) -> String {
    if state.last_notify_ok == 0 {
        "and none has ever arrived".to_string()
    } else {
        format!(
            "the last one that arrived was {} h earlier",
            state
                .last_notify_failed
                .saturating_sub(state.last_notify_ok)
                / 3600
        )
    }
}

/// G8: a stack whose last deploy stopped halfway, reported out loud.
///
/// `incomplete_step` has been written since S2 and read by nobody. That is
/// the exact shape this whole audit keeps finding: a mechanism that runs,
/// records the truth, and is wired to nothing. The record was added because
/// the media stack failed at "start apps" and therefore did not exist as far
/// as the orchestrator was concerned — 12 GB of configuration with no nightly
/// backup, and nothing anywhere saying so. Writing the field fixed the
/// backup; it did not make anyone aware. This does.
///
/// Severity is Broken rather than Drift on purpose: a half-applied stack is
/// not a container that will bite later, it is one whose running state nobody
/// has claimed. The remedy names the step, because "the deploy failed" a week
/// ago is not something Kenny can act on and "it stopped at start apps" is.
pub fn evaluate_incomplete(state: &HostState) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(step) = st.incomplete_step.as_ref() else {
            continue;
        };
        out.push(Finding {
            severity: Severity::Broken,
            subject: name.clone(),
            what: format!("its last deploy stopped at \"{}\" and never finished", step),
            remedy: "run the deploy again and watch that step — until it completes, what runs \
                     on the container and what the orchestrator has on record are two different \
                     things, and only the record drives drift detection and retention"
                .into(),
        });
    }
    out.sort_by(|a, b| a.subject.cmp(&b.subject));
    out
}
