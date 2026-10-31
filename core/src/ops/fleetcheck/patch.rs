//! Updates that stand still: reboots pending and unattended-upgrades stalled.

use super::{Finding, Severity};

/// fix-150 (expert panel 2026-09-27, the patch-state half of
/// check-blind-to-repo-drift; Kenny 2026-09-28: "7 dagen"): how long a
/// container's updates may stand before `homelab check` says so.
pub const PATCH_THRESHOLD_S: u64 = 7 * 86_400;

/// fix-150: one managed container's patch state, as its probe reported it.
/// `upgradable` None = the probe did not answer (a stopped container, a
/// broken exec); then nothing is judged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatchFact {
    pub vmid: u16,
    pub hostname: String,
    /// Packages `apt` would upgrade right now.
    pub upgradable: Option<u32>,
    /// How long `/var/run/reboot-required` has existed; None = no reboot
    /// pending.
    pub reboot_required_age_s: Option<u64>,
    /// How long ago unattended-upgrades last ran to completion
    /// (`/var/lib/apt/periodic/upgrade-stamp`); None = never on this
    /// container.
    pub unattended_stamp_age_s: Option<u64>,
    /// How old the container is (`/etc/hostname`, written by Proxmox when it
    /// creates the container); None = unknown, judged as old.
    pub age_s: Option<u64>,
}

/// fix-150: updates that stand still. Two things count, both against
/// [`PATCH_THRESHOLD_S`]: a reboot that has been required for longer, and a
/// container whose daily unattended-upgrades run has not completed for
/// longer (or ever). Upgradable packages on their own are not a finding:
/// the nightly `homelab patch` takes them, and on 2026-09-27 21:15 CT 116
/// held 40 and CT 113 64 with everything working — the count is named in
/// the finding, never the reason for it. Drift, not Broken: nothing is down.
pub fn evaluate_patch_state(facts: &[PatchFact], threshold_s: u64) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in facts {
        let Some(upgradable) = f.upgradable else {
            continue;
        };
        let subject = format!("{} ({})", f.vmid, f.hostname);
        out.extend(reboot_finding(f, &subject, upgradable, threshold_s));
        out.extend(stamp_finding(f, subject, upgradable, threshold_s));
    }
    out
}

fn days(s: u64) -> u64 {
    s / 86_400
}

/// A reboot that has been required for longer than the threshold.
fn reboot_finding(
    f: &PatchFact,
    subject: &str,
    upgradable: u32,
    threshold_s: u64,
) -> Option<Finding> {
    let age = f.reboot_required_age_s.filter(|a| *a > threshold_s)?;
    Some(Finding {
        severity: Severity::Drift,
        subject: subject.to_string(),
        what: format!(
                        "has needed a reboot for {} days (a kernel or libc update is installed                          but not running); {} package(s) upgradable",
            days(age),
            upgradable
        ),
        remedy: "reboot the container outside the backup hour: `pct reboot <vmid>` on                              pve, or restart it from the TUI"
            .into(),
    })
}

/// A container whose daily unattended-upgrades run has not completed for
/// longer than the threshold, or never.
fn stamp_finding(
    f: &PatchFact,
    subject: String,
    upgradable: u32,
    threshold_s: u64,
) -> Option<Finding> {
    match f.unattended_stamp_age_s {
        Some(age) if age > threshold_s => Some(Finding {
            severity: Severity::Drift,
            subject,
            what: format!(
                "unattended-upgrades last completed {} days ago; {} package(s) upgradable",
                days(age),
                upgradable
            ),
            remedy: "inside the container: `systemctl status apt-daily-upgrade.timer` and                          `unattended-upgrade -d`; the golden template carries the working                          configuration"
                .into(),
        }),
        // A container younger than the threshold has not had its week
        // yet (the first check after the 2026-09-28 rebuilds flagged
        // seven containers built that morning).
        None if f.age_s.is_some_and(|a| a <= threshold_s) => None,
        None => Some(Finding {
            severity: Severity::Drift,
            subject,
            what: format!(
                "unattended-upgrades has never completed a run here; {} package(s) upgradable",
                upgradable
            ),
            remedy: "inside the container: `apt install unattended-upgrades` and                          `systemctl enable --now apt-daily-upgrade.timer`, as the golden                          template has"
                .into(),
        }),
        Some(_) => None,
    }
}
