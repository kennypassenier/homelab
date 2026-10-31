//! Every recorded stack's firewall file on pve, held against its declaration.

use super::firewall_live::TileInputs;
use super::{BootFact, Finding, FirewallFact, Severity};
use crate::manifest::FirewallSpec;
use crate::manifest::StackManifest;
use crate::state::HostState;

/// fix-88: every recorded stack's firewall file on pve, held against the
/// declaration the stack's last deploy recorded.
///
/// Drift, not Broken: a container without its rules still serves, and the
/// point is to see a hand edit or a missing file before it matters. The one
/// state that is neither a fault nor silent is a declaration kept for the
/// rollout — noted in a single line, so the list of stacks still to switch
/// on stays visible without a finding per stack every night.
pub fn evaluate_firewalls(
    state: &HostState,
    files: &[FirewallFact],
    boot: &[BootFact],
    tile_watch_source: Option<&str>,
    trusted: &[String],
) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut not_enabled: Vec<String> = Vec::new();
    let inputs = TileInputs::new(state, tile_watch_source, trusted);
    for f in files {
        let path = crate::firewall::fw_path(f.vmid);
        let subject = format!("{} ({})", path, f.stack);
        let manifest = state.stacks.get(&f.stack).and_then(|s| s.manifest.as_ref());
        let decl = manifest.and_then(|m| m.firewall.as_ref());
        match (decl, f.content.as_deref()) {
            (Some(d), content) if d.enabled => {
                out.extend(file_finding(&inputs, f, &subject, d, manifest, content));
                out.extend(nic_finding(f, boot));
            }
            (_, Some(_)) => out.push(undeclared_ruleset(f, subject)),
            (Some(_), None) => not_enabled.push(f.stack.clone()),
            (None, None) => {}
        }
    }
    out.extend(rollout_note(not_enabled));
    out
}

/// The declared firewall against pve's file: absent, or differing.
fn file_finding(
    inputs: &TileInputs,
    f: &FirewallFact,
    subject: &str,
    declared: &FirewallSpec,
    manifest: Option<&StackManifest>,
    content: Option<&str>,
) -> Option<Finding> {
    let want = inputs.wanted(&f.stack, declared, manifest);
    match content {
        None => Some(Finding {
            severity: Severity::Drift,
            subject: subject.to_string(),
            what: "the stack declares a firewall, but the file is absent — the \
                   container runs without its rules"
                .into(),
            remedy: format!("deploy stacks/{}; it writes the file", f.stack),
        }),
        Some(have) if have != want => {
            let (extra, missing) = crate::firewall::line_changes(&want, have);
            Some(Finding {
                severity: Severity::Drift,
                subject: subject.to_string(),
                what: format!(
                    "differs from its declaration — on pve only: {}; declared only: {}",
                    show_lines(&extra),
                    show_lines(&missing)
                ),
                remedy: format!(
                    "a hand edit on pve: put the change in the `firewall:` block of \
                     stacks/{}/lxc-compose.yml and deploy, or deploy as it is to put \
                     the declaration back",
                    f.stack
                ),
            })
        }
        Some(_) => None,
    }
}

/// At most three differing lines, then a count of the rest.
fn show_lines(v: &[String]) -> String {
    let mut s = v.iter().take(3).cloned().collect::<Vec<_>>().join(" | ");
    if v.len() > 3 {
        s.push_str(&format!(" | … {} more", v.len() - 3));
    }
    if s.is_empty() { "nothing".into() } else { s }
}

/// The NIC flag that stops Proxmox applying the declared rules at all.
fn nic_finding(f: &FirewallFact, boot: &[BootFact]) -> Option<Finding> {
    boot.iter()
        .any(|b| b.vmid == f.vmid && b.live.nic_firewall == Some(false))
        .then(|| Finding {
            severity: Severity::Drift,
            subject: format!("{} ({})", f.vmid, f.stack),
            what: "net0 has firewall=0, so Proxmox applies none of the declared rules".into(),
            remedy: format!("deploy stacks/{}; it switches the NIC flag on", f.stack),
        })
}

/// A file on pve for a stack whose declaration does not enable a firewall.
fn undeclared_ruleset(f: &FirewallFact, subject: String) -> Finding {
    Finding {
        severity: Severity::Drift,
        subject,
        what: "exists on pve, but the stack file does not enable a firewall — a \
               ruleset the repository does not know"
            .into(),
        remedy: format!(
            "declare it in the `firewall:` block of stacks/{}/lxc-compose.yml with \
             `enabled: true` and deploy, so the deploy owns the file; or remove the file \
             on pve if it should not exist",
            f.stack
        ),
    }
}

/// The one line that keeps the list of stacks still to switch on visible.
fn rollout_note(mut not_enabled: Vec<String>) -> Option<Finding> {
    if not_enabled.is_empty() {
        return None;
    }
    not_enabled.sort();
    Some(Finding {
        severity: Severity::Noted,
        subject: "firewall".into(),
        what: format!("declared, not enabled yet: {}", not_enabled.join(", ")),
        remedy: "switch one stack on at a time: `enabled: true` in its `firewall:` block, \
                 deploy it, and watch its checks and monitors"
            .into(),
    })
}
