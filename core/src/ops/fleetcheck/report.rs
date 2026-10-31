//! Turning findings into an answer: what alarms, what passes, what is sent,
//! and the text `homelab check` prints.

use super::{Finding, Severity};

/// Which findings are worth waking somebody for.
///
/// Z3: a `Noted` finding is a decision, not a fault — the registry cache
/// that is deliberately not backed up, a mount declared unkept. Letting one
/// raise the alarm trains the reader to ignore the notification, and that
/// notification is the one that has to be believed when it IS real.
///
/// Extracted from the nightly loop on 2026-09-02 (G15 of the Phase-7 gate).
/// It lived inside a 340-line async function that no test could reach, which
/// meant the rule deciding what counts as an alarm was itself unguarded.
pub fn alarming(findings: &[Finding]) -> Vec<Finding> {
    findings
        .iter()
        .filter(|f| f.severity != Severity::Noted)
        .cloned()
        .collect()
}

/// fix-65 (nightly-report-always-red, 2026-09-27): which things are alarming,
/// as one comparable string. Severity and subject only: the wording carries
/// ages and counts that move every night, and a fingerprint that moves every
/// night would send the same report every night — the failure this exists
/// to end.
pub fn report_fingerprint(findings: &[Finding]) -> String {
    let mut keys: Vec<String> = alarming(findings)
        .iter()
        .map(|f| format!("{:?}|{}", f.severity, f.subject))
        .collect();
    keys.sort();
    keys.dedup();
    keys.join("\n")
}

/// fix-65: does tonight's alarming set go out as a notification? The same
/// red report every night taught its reader to ignore it, and a real new
/// problem then arrived in that envelope. It goes out when the set changed
/// since the last one sent, and once a week while it stands.
pub fn nightly_report_due(
    fingerprint: &str,
    last_fingerprint: &str,
    last_sent: u64,
    now: u64,
    repeat_s: u64,
) -> bool {
    fingerprint != last_fingerprint || now.saturating_sub(last_sent) >= repeat_s
}

/// fix-65: how often a standing alarming set is sent again.
pub const NIGHTLY_REPORT_REPEAT_S: u64 = 7 * 24 * 3600;

/// Whether `homelab check` passes: nothing alarming (gap-32). `noted`
/// findings need nothing done and are printed only so they stay visible, so
/// they no longer turn the answer into a failure (exit 1).
pub fn check_passes(findings: &[Finding]) -> bool {
    alarming(findings).is_empty()
}

/// The check as text, for `homelab check`, the TUI and the nightly log.
///
/// fix-103: a summary line comes first, then each severity as its own
/// group, the one that needs action on top. The `[broken]`/`[drift]`/
/// `[noted]` tags stay on every item, so a client can colour it and a grep
/// still finds it. Story: `docs/deployment/REGISTER.md`.
pub fn render(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "fleet check: repo and reality agree".into();
    }
    let mut out = format!("fleet check: {}\n", summary(findings));
    for (severity, tag, meaning) in GROUPS {
        out.push_str(&group_section(findings, severity, tag, meaning));
    }
    out
}

/// Each severity with its tag and what the tag means, the one that needs
/// action on top.
const GROUPS: [(Severity, &str, &str); 3] = [
    (Severity::Broken, "broken", "not doing its job now"),
    (
        Severity::Drift,
        "drift",
        "works, bites on the next deploy or outage",
    ),
    (Severity::Noted, "noted", "nothing to do"),
];

/// The count per severity, as the first line of the report.
fn summary(findings: &[Finding]) -> String {
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let parts: Vec<String> = GROUPS
        .iter()
        .filter(|(s, _, _)| count(*s) > 0)
        .map(|(s, tag, _)| match s {
            Severity::Noted => format!("{} {} (nothing to do)", count(*s), tag),
            _ => format!("{} {}", count(*s), tag),
        })
        .collect();
    parts.join(" · ")
}

/// One severity's heading and its findings; empty when it has none.
fn group_section(findings: &[Finding], severity: Severity, tag: &str, meaning: &str) -> String {
    let items: Vec<&Finding> = findings.iter().filter(|f| f.severity == severity).collect();
    if items.is_empty() {
        return String::new();
    }
    let mut out = format!("{} — {}:\n", tag, meaning);
    for f in items {
        out.push_str(&format!(
            "  [{}] {} — {}\n      remedy: {}\n",
            tag, f.subject, f.what, f.remedy
        ));
    }
    out
}
