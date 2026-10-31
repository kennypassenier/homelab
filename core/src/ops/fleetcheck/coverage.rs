//! Is each stack measured and shipping logs?

use super::{Finding, Severity};

/// Is this stack actually being measured and actually shipping logs?
///
/// The most expensive class of failure in this fleet is not a service that
/// falls over — it is a mechanism that runs, reports success and is wired to
/// nothing. On 2026-08-31 alone: log caps that ran on five of nine
/// containers, a growth check that watched five of nine, a discovery file the
/// orchestrator wrote for weeks that Prometheus was never told to read, a
/// promtail pipeline reading a field docker does not write, a database
/// answering its healthcheck while every query failed, and an alert chain
/// finished on every side but the middle. Not one was caught by a test. Every
/// one was found by somebody looking.
///
/// So the check looks. Both fields are `Option`: `None` means the question
/// was not asked — no Prometheus or Loki address is configured, or the stack
/// ships no logs by design — and an unasked question must never become a
/// finding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoverageFact {
    pub stack: String,
    /// A Prometheus target for this stack answered `up == 1`.
    pub scraped: Option<bool>,
    /// Loki holds at least one line from this stack, carrying a container
    /// name, inside the configured window.
    ///
    /// Labelled, because F79 was not silence. Promtail read `attrs.name` —
    /// a field docker does not write — so lines arrived for months with an
    /// empty `container_name` and the three dashboards querying it stayed
    /// blank. A plain line count would have been green throughout.
    ///
    /// The window is a host setting (`logs_window`, default 24h) rather than
    /// the hour it started as. On 2026-09-01 the hour version reported
    /// `home` and `kp-soft` as "going nowhere" while both were healthy and
    /// merely quiet — a check that alarms on healthy silence is a check that
    /// gets switched off, and then the real silence goes unnoticed too.
    pub logs_recent: Option<bool>,
    /// Its service.yml says `metrics: false`: deliberately not measured, so
    /// Prometheus is not asked and the check notes it (Phase 9, inbox).
    pub unmeasured_by_choice: bool,
}

/// Is each stack's safety net attached? See `CoverageFact` for why this
/// exists at all.
///
/// Drift rather than Broken throughout: nothing is failing: the stack runs
/// fine. What is missing is the ability to find out when it stops, which is a
/// slower and more expensive kind of wrong.
pub fn evaluate_coverage(facts: &[CoverageFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in facts {
        if c.unmeasured_by_choice {
            out.push(Finding {
                severity: Severity::Noted,
                subject: c.stack.clone(),
                what:
                    "deliberately not measured by Prometheus (`metrics: false` in its service.yml)"
                        .into(),
                remedy: "nothing, unless that decision changes: remove the line and add a target"
                    .into(),
            });
        }
        if c.scraped == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no Prometheus target answers for this stack — it is not being measured".into(),
                remedy: "check /appdata/metrics/prometheus-config/targets/<stack>.json exists and that Prometheus reads that directory".into(),
            });
        }
        if c.logs_recent == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no LABELLED log line reached Loki from this stack recently — either nothing is shipping, or it ships without the container name".into(),
                remedy: "on that container: `systemctl status alloy`, then `runuser -u alloy -- ls /var/lib/docker/containers` (a compose stack) or `journalctl -u alloy` (a native one); a redeploy re-applies Alloy's config and its read access".into(),
            });
        }
    }
    out
}
