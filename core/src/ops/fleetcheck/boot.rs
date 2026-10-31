//! A container's boot policy and resources against its stack file.

use super::{Finding, Severity};
use crate::state::HostState;

/// W3: the configured shape of a container that exists, next to the stack
/// file that is supposed to describe it.
///
/// Read from `pct config` rather than from inside the container, so a
/// stopped guest answers as well as a running one — which matters, because
/// "does not start on boot" is precisely the state you find a container in
/// after the reboot that should have started it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BootFact {
    pub vmid: u16,
    pub hostname: String,
    pub live: crate::ops::reconcile::LiveConfig,
}

/// W3: report a container whose boot policy or resources no longer match the
/// stack file. Boot policy is a drift a deploy repairs on its own; memory and
/// cores are named with the remedy that actually applies, because raising
/// them is a deliberate operation and lowering them is a rebuild.
///
/// A stack whose state record carries no manifest is skipped rather than
/// guessed at — the same rule as everywhere else here: a question that was
/// not asked never becomes a finding.
pub fn evaluate_boot(state: &HostState, facts: &[BootFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(fact) = facts.iter().find(|f| f.vmid == st.vmid) else {
            continue;
        };
        for d in crate::ops::reconcile::divergences(m, &fact.live) {
            let boot_related = d.starts_with("starts on boot") || d.starts_with("boot order");
            out.push(Finding {
                severity: Severity::Drift,
                subject: name.clone(),
                what: d,
                remedy: if boot_related {
                    "a deploy puts the boot policy back; until then a reboot starts the fleet in the wrong order".into()
                } else {
                    "raise it with `homelab resize`, or lower it by rebuilding the container — a deploy deliberately does not change resources under a running service".into()
                },
            });
        }
    }
    out
}
